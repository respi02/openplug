package com.openplug

import android.content.Context
import android.content.Intent
import android.os.PowerManager
import android.util.Log
import androidx.core.content.ContextCompat
import com.openplug.core.Dongle
import com.openplug.core.DongleException
import com.openplug.core.JournalException
import com.openplug.core.JournalTotals
import com.openplug.core.LastReading
import com.openplug.core.ParkedAt
import com.openplug.core.RoutePoint
import com.openplug.core.Reading
import com.openplug.core.RecordingException
import com.openplug.core.RefuelRecord
import com.openplug.core.TripJournal
import com.openplug.core.TripRecord
import com.openplug.core.ValueRecorder
import com.openplug.transport.DongleTransport
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

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
    /** The last known value of every data object, for showing the car while it is away. */
    val last: List<LastReading> = emptyList(),
    /** True while a trip is being recorded right now. */
    val tripRunning: Boolean = false,
    /** Where the car was when the link last dropped. */
    val parked: ParkedAt? = null,
)

/**
 * The one dongle session of the process. It runs on a thread owned by
 * [DongleService] (a foreground service, so it survives the app going to the
 * background while driving) and is observed by the UI through the flows here.
 *
 * Once started it keeps the link up: when the dongle goes away (ignition off, out
 * of range) it waits for it and reconnects, until [stop] is called or the PIN is
 * rejected. Waiting survives the phone sleeping ([Presence] wakes it when the dongle
 * advertises), the process being killed ([DongleService] is sticky) and a reboot
 * ([RestartReceiver]), because the wish to stay connected is stored in [Settings].
 */
object Session {
    private const val TAG = "OpenPlug"

    private val _state = MutableStateFlow<LinkState>(LinkState.Idle)
    val state: StateFlow<LinkState> = _state

    private val _readings = MutableStateFlow<List<Reading>>(emptyList())
    val readings: StateFlow<List<Reading>> = _readings

    private val _journal = MutableStateFlow(JournalSnapshot())
    val journal: StateFlow<JournalSnapshot> = _journal

    /** Set if the trip log on disk could not be opened; trips are then not recorded. */
    private val _journalProblem = MutableStateFlow<String?>(null)
    val journalProblem: StateFlow<String?> = _journalProblem

    /** The file the current session records its values to, if recording is on. */
    private val _recording = MutableStateFlow<String?>(null)
    val recording: StateFlow<String?> = _recording

    private class Request(val settings: SessionSettings, val open: () -> DongleTransport, val recordTo: File?)

    private val lock = Object()
    private var trips: TripJournal? = null
    /** Which journal is open: null = none yet, true = the fake dongle's (debug builds). */
    private var journalIsDemo: Boolean? = null
    private var pending: Request? = null
    private var running = false
    private var stopped = false
    private var kicked = false
    private var transport: DongleTransport? = null
    private var wakeLock: PowerManager.WakeLock? = null

    val isActive: Boolean get() = synchronized(lock) { running || pending != null }

    /** Makes sure a trip log is open (the car's, unless one is open already). */
    fun openJournal(context: Context) {
        if (synchronized(lock) { journalIsDemo } == null) useJournal(context, demo = false)
    }

    /**
     * Opens the car's trip log, or the fake dongle's, which debug builds keep apart so
     * synthetic values never end up in the car's log. Only called while no session runs.
     */
    private fun useJournal(context: Context, demo: Boolean) {
        synchronized(lock) {
            if (journalIsDemo == demo) return
            journalIsDemo = demo
        }
        trips?.destroy()
        trips = null
        try {
            val name = if (demo) "journal-demo.json" else "journal.json"
            val journal = TripJournal.open(File(context.filesDir, name).path)
            journal.tick(now()) // close a trip left paused by an earlier run
            trips = journal
            _journalProblem.value = null
            publishJournal()
        } catch (e: JournalException) {
            _journalProblem.value = e.message
            _journal.value = JournalSnapshot()
        }
    }

    /** Where recordings go: reachable with `adb pull`, no storage permission needed. */
    fun recordingsDir(context: Context): File =
        File(context.getExternalFilesDir(null) ?: context.filesDir, "recordings")

    /**
     * Starts a session in the foreground service and remembers that one is wanted.
     * With `record`, the value stream of the whole session is also written to a new
     * file in [recordingsDir], to replay the drive later.
     */
    fun start(context: Context, settings: SessionSettings) {
        val app = context.applicationContext
        Settings.save(app, settings)
        Settings.setSessionWanted(app, true)
        if (prepare(app, settings)) {
            ContextCompat.startForegroundService(app, Intent(app, DongleService::class.java))
        }
    }

    /**
     * Picks up a wanted session that is not running (after the process was killed, a
     * reboot, or when the dongle shows up). Returns false if there is nothing to resume
     * or the system does not let the app start in the background.
     */
    fun resume(context: Context): Boolean {
        val app = context.applicationContext
        if (isActive) return true
        if (!Settings.isSessionWanted(app)) return false
        val settings = Settings.saved(app) ?: return false
        if (!prepare(app, settings)) return false
        return try {
            ContextCompat.startForegroundService(app, Intent(app, DongleService::class.java))
            true
        } catch (e: IllegalStateException) {
            // ForegroundServiceStartNotAllowedException: not exempt from battery optimization
            Log.w(TAG, "cannot resume from the background: $e")
            synchronized(lock) { pending = null }
            false
        }
    }

    /** For a sticky restart of [DongleService]: queue the wanted session, if any. */
    internal fun prepareResume(context: Context): Boolean {
        if (isActive) return true
        if (!Settings.isSessionWanted(context)) return false
        val settings = Settings.saved(context) ?: return false
        return prepare(context.applicationContext, settings)
    }

    private fun prepare(app: Context, settings: SessionSettings): Boolean {
        val open = Settings.transportFor(app, settings.targetId) ?: return false
        val recordTo = if (settings.record) {
            val stamp = SimpleDateFormat("yyyyMMdd-HHmmss", Locale.ROOT).format(Date())
            File(recordingsDir(app), "drive-$stamp.jsonl")
        } else {
            null
        }
        if (isActive) return false
        useJournal(app, settings.isFake)
        synchronized(lock) {
            if (running || pending != null) return false
            pending = Request(settings, open, recordTo)
            stopped = false
            kicked = false
        }
        _readings.value = emptyList()
        _state.value = LinkState.Connecting("Connecting…")
        return true
    }

    /** Drops a queued session that could not be started (the wish to run stays stored). */
    internal fun abandonPending() {
        synchronized(lock) { pending = null }
        if (!synchronized(lock) { running }) _state.value = LinkState.Idle
    }

    /** Stops the session for good (no more retries, nothing resumes it). */
    fun stop(context: Context) {
        val app = context.applicationContext
        Settings.setSessionWanted(app, false)
        Presence.stop(app)
        val t = synchronized(lock) {
            stopped = true
            pending = null
            lock.notifyAll() // ends a wait between retries
            transport
        }
        t?.close() // unblocks the session thread
        if (!synchronized(lock) { running }) _state.value = LinkState.Idle
    }

    /** The dongle was seen nearby: stop waiting and connect now. */
    fun kick(context: Context) {
        // long enough for the connect attempt (a connected session holds its own)
        if (_state.value !is LinkState.Connected) stayAwake(context, 60_000)
        synchronized(lock) {
            kicked = true
            lock.notifyAll()
        }
    }

    /** Runs the session on the calling thread; returns when it is over. */
    internal fun run(context: Context) {
        val app = context.applicationContext
        val request = synchronized(lock) {
            val r = pending ?: return
            pending = null
            running = true
            r
        }
        val recorder = request.recordTo?.let { file ->
            try {
                ValueRecorder.open(file.path, now()).also { _recording.value = file.path }
            } catch (e: RecordingException) {
                null // record nothing rather than not connect
            }
        }
        val end = try {
            loop(app, request, recorder)
        } finally {
            recorder?.destroy()
            _recording.value = null
            Presence.stop(app)
            letSleep()
            synchronized(lock) {
                running = false
                transport = null
            }
        }
        if (end is LinkState.Failed) Settings.setSessionWanted(app, false) // retrying cannot help
        publishJournal()
        _state.value = end
    }

    private fun isStopped() = synchronized(lock) { stopped }

    private fun loop(app: Context, request: Request, recorder: ValueRecorder?): LinkState {
        val bluetooth = !request.settings.isFake
        var failures = 0
        while (true) {
            val t = request.open()
            synchronized(lock) {
                if (stopped) return LinkState.Idle
                transport = t
                kicked = false
            }
            var wasConnected = false
            val problem = try {
                stream(app, t, request.settings.pin, recorder) {
                    wasConnected = true
                    Presence.stop(app) // the dongle stops advertising while we hold it
                }
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
                recorder?.onDisconnected(now())
            }
            publishJournal()
            if (isStopped()) return LinkState.Idle

            // Wait for the dongle. Timed retries back off (5 s, 10 s, 20 s, then every
            // 30 s) but stall while the phone sleeps; the presence scan cuts the wait
            // short as soon as the dongle advertises.
            if (bluetooth) Presence.start(app)
            letSleep()
            failures += 1
            val delay = minOf(30, 5 * (1 shl minOf(failures - 1, 3)))
            for (left in delay downTo 1) {
                if (isStopped()) return LinkState.Idle
                _state.value = LinkState.Waiting("$problem Trying again in $left s…")
                if (waitForKick(1000)) break
                if (trips?.tick(now()) != null) publishJournal()
            }
            if (isStopped()) return LinkState.Idle
            _state.value = LinkState.Connecting("Connecting…")
        }
    }

    /** Waits up to [ms]; true if [kick] was called meanwhile. */
    private fun waitForKick(ms: Long): Boolean = synchronized(lock) {
        if (!kicked && !stopped) lock.wait(ms)
        kicked.also { kicked = false }
    }

    /** One connection: authenticate, subscribe, pump readings until the link ends. */
    private fun stream(
        app: Context,
        t: DongleTransport,
        pin: String,
        recorder: ValueRecorder?,
        onConnected: () -> Unit,
    ) {
        t.open()
        _state.value = LinkState.Connecting("Authenticating…")
        try {
            pump(app, t, pin, recorder, onConnected)
        } finally {
            PositionFeed.stop(app)
        }
    }

    private fun pump(
        app: Context,
        t: DongleTransport,
        pin: String,
        recorder: ValueRecorder?,
        onConnected: () -> Unit,
    ) {
        Dongle.connect(t, pin).use { dongle ->
            dongle.subscribeDefaults()
            onConnected()
            trips?.onConnected(now())
            recorder?.onConnected(now())
            _state.value = LinkState.Connected
            PositionFeed.start(app)
            val latest = LinkedHashMap<UInt, Reading>()
            var awakeUntil = 0L
            while (true) {
                val batch = dongle.nextReadings()
                // keep the CPU up while connected, so no reading waits for the phone to wake
                val nowMs = System.currentTimeMillis()
                if (nowMs > awakeUntil - 5 * 60_000) {
                    stayAwake(app, 10 * 60_000)
                    awakeUntil = nowMs + 10 * 60_000
                }
                recorder?.onReadings(batch, now())
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

    /** Holds a partial wake lock for [ms] from now (replacing any earlier timeout). */
    private fun stayAwake(context: Context, ms: Long) {
        val lock = synchronized(this.lock) {
            wakeLock ?: context.applicationContext.getSystemService(PowerManager::class.java)
                .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "openplug:session")
                .apply { setReferenceCounted(false) }
                .also { wakeLock = it }
        }
        lock.acquire(ms)
    }

    private fun letSleep() {
        synchronized(lock) { wakeLock }?.let { if (it.isHeld) it.release() }
    }

    /** The route of the trip that started at [startMs]; empty if none was recorded. */
    fun route(startMs: ULong): List<RoutePoint> = trips?.track(startMs) ?: emptyList()

    /** A position fix from the phone, while the car is connected. */
    internal fun onPosition(lat: Double, lon: Double) {
        trips?.onPosition(lat, lon, now())
    }

    /** The finished trips as CSV text, or null when there is no trip log. */
    fun exportCsv(): String? = trips?.exportCsv()

    private fun publishJournal() {
        val j = trips ?: return
        _journal.value = JournalSnapshot(j.activeTrip(), j.trips(), j.refuels(), j.totals(), j.lastValues(), j.tripRunning(), j.parked())
    }

    private fun now(): ULong = System.currentTimeMillis().toULong()
}
