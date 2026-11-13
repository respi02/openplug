package com.openplug

import android.content.Context
import android.content.Intent
import androidx.core.content.ContextCompat
import com.openplug.core.Dongle
import com.openplug.core.DongleException
import com.openplug.core.JournalException
import com.openplug.core.JournalTotals
import com.openplug.core.Reading
import com.openplug.core.RefuelRecord
import com.openplug.core.TripJournal
import com.openplug.core.TripRecord
import com.openplug.transport.DongleTransport
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import java.io.File

sealed interface LinkState {
    data object Idle : LinkState
    data class Connecting(val what: String) : LinkState
    data object Connected : LinkState

    /** The link dropped (or the dongle did not answer); a retry is scheduled. */
    data class Waiting(val what: String) : LinkState
    data class Failed(val message: String) : LinkState
}

/** What the trip journal currently holds. */
data class JournalSnapshot(
    val active: TripRecord? = null,
    val trips: List<TripRecord> = emptyList(),
    val refuels: List<RefuelRecord> = emptyList(),
    val totals: JournalTotals = JournalTotals(0u, 0.0, 0uL, 0.0),
)

/**
 * The one dongle session of the process. It runs on a thread owned by
 * [DongleService] (a foreground service, so it survives the app going to the
 * background while driving) and is observed by the UI through the flows here.
 *
 * Once started it keeps the link up: when the dongle goes away (ignition off, out
 * of range) it retries until [stop] is called or the PIN is rejected.
 */
object Session {
    private val _state = MutableStateFlow<LinkState>(LinkState.Idle)
    val state: StateFlow<LinkState> = _state

    private val _readings = MutableStateFlow<List<Reading>>(emptyList())
    val readings: StateFlow<List<Reading>> = _readings

    private val _journal = MutableStateFlow(JournalSnapshot())
    val journal: StateFlow<JournalSnapshot> = _journal

    /** Set if the trip log on disk could not be opened; trips are then not recorded. */
    private val _journalProblem = MutableStateFlow<String?>(null)
    val journalProblem: StateFlow<String?> = _journalProblem

    private class Request(val open: () -> DongleTransport, val pin: String)

    private val lock = Any()
    private var trips: TripJournal? = null
    private var journalOpened = false
    private var pending: Request? = null
    private var running = false
    private var stopped = false
    private var transport: DongleTransport? = null

    val isActive: Boolean get() = synchronized(lock) { running || pending != null }

    /** Opens the trip log; safe to call repeatedly. */
    fun openJournal(context: Context) {
        synchronized(lock) {
            if (journalOpened) return
            journalOpened = true
        }
        try {
            val journal = TripJournal.open(File(context.filesDir, "journal.json").path)
            journal.tick(now()) // close a trip left paused by an earlier run
            trips = journal
            publishJournal()
        } catch (e: JournalException) {
            _journalProblem.value = e.message
        }
    }

    /** Starts the session in the foreground service. */
    fun start(context: Context, open: () -> DongleTransport, pin: String) {
        synchronized(lock) {
            if (running || pending != null) return
            pending = Request(open, pin)
            stopped = false
        }
        _readings.value = emptyList()
        _state.value = LinkState.Connecting("Connecting…")
        ContextCompat.startForegroundService(context, Intent(context, DongleService::class.java))
    }

    /** Stops the session for good (no more retries). */
    fun stop() {
        val t = synchronized(lock) {
            stopped = true
            pending = null
            transport
        }
        t?.close() // unblocks the session thread
        if (!synchronized(lock) { running }) _state.value = LinkState.Idle
    }

    /** Runs the session on the calling thread; returns when it is over. */
    internal fun run() {
        val request = synchronized(lock) {
            val r = pending ?: return
            pending = null
            running = true
            r
        }
        val end = try {
            loop(request)
        } finally {
            synchronized(lock) {
                running = false
                transport = null
            }
        }
        publishJournal()
        _state.value = end
    }

    private fun isStopped() = synchronized(lock) { stopped }

    private fun loop(request: Request): LinkState {
        var failures = 0
        while (true) {
            val t = request.open()
            synchronized(lock) {
                if (stopped) return LinkState.Idle
                transport = t
            }
            var wasConnected = false
            val problem = try {
                stream(t, request.pin) { wasConnected = true }
                "The connection ended."
            } catch (e: DongleException.WrongPin) {
                t.close()
                return LinkState.Failed("The dongle rejected the PIN.")
            } catch (e: DongleException.Closed) {
                "The dongle closed the connection."
            } catch (e: SecurityException) {
                t.close()
                return LinkState.Failed("Bluetooth permission is missing.")
            } catch (e: Exception) {
                if (wasConnected) "Connection lost." else "The dongle is not answering."
            }
            t.close()
            if (wasConnected) {
                failures = 0
                trips?.onDisconnected(now())
            }
            publishJournal()
            if (isStopped()) return LinkState.Idle

            // Back off: 5 s, 10 s, 20 s, then every 30 s.
            failures += 1
            val delay = minOf(30, 5 * (1 shl minOf(failures - 1, 3)))
            for (left in delay downTo 1) {
                if (isStopped()) return LinkState.Idle
                _state.value = LinkState.Waiting("$problem Trying again in $left s…")
                Thread.sleep(1000)
                if (trips?.tick(now()) != null) publishJournal()
            }
            _state.value = LinkState.Connecting("Connecting…")
        }
    }

    /** One connection: authenticate, subscribe, pump readings until the link ends. */
    private fun stream(t: DongleTransport, pin: String, onConnected: () -> Unit) {
        t.open()
        _state.value = LinkState.Connecting("Authenticating…")
        Dongle.connect(t, pin).use { dongle ->
            dongle.subscribeDefaults()
            onConnected()
            trips?.onConnected(now())
            _state.value = LinkState.Connected
            val latest = LinkedHashMap<UInt, Reading>()
            while (true) {
                val batch = dongle.nextReadings()
                for (r in batch) latest[r.url] = r
                // labelled values first, then by id
                _readings.value = latest.values.sortedWith(compareBy<Reading> { it.name == null }.thenBy { it.url })
                trips?.let {
                    it.onReadings(batch, now())
                    publishJournal()
                }
            }
        }
    }

    private fun publishJournal() {
        val j = trips ?: return
        _journal.value = JournalSnapshot(j.activeTrip(), j.trips(), j.refuels(), j.totals())
    }

    private fun now(): ULong = System.currentTimeMillis().toULong()
}
