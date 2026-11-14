package com.openplug

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log
import kotlin.concurrent.thread

/**
 * Keeps the dongle session alive while the app is in the background, so a drive is
 * recorded from start to end. Sticky: if the system kills the process, the service
 * comes back and resumes the session.
 *
 * Its ongoing notification follows the session: the trip in progress (speed, distance,
 * time driving) while connected, otherwise what the session is waiting for. On
 * Android 16+ it asks to be promoted to a Live Update (status-bar chip, lock screen);
 * older versions show it as an ordinary ongoing notification.
 */
class DongleService : Service() {

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // A sticky restart after the process was killed arrives with no work queued:
        // pick up the wanted session, or stop.
        val restarted = intent == null
        if (restarted && !Session.prepareResume(this)) {
            stopSelf()
            return START_NOT_STICKY
        }
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, "Dongle connection", NotificationManager.IMPORTANCE_LOW),
        )
        val content = Content.now()
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                startForeground(NOTIFICATION_ID, notification(content), ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
            } else {
                startForeground(NOTIFICATION_ID, notification(content))
            }
        } catch (e: RuntimeException) {
            // SecurityException without the Bluetooth permission; on a restart from the
            // background, ForegroundServiceStartNotAllowedException unless the app is
            // exempt from battery optimization. Leave the session wanted: the presence
            // scan or the next app start picks it up.
            Log.w("OpenPlug", "cannot run in the foreground: $e")
            Session.abandonPending()
            stopSelf()
            return START_NOT_STICKY
        }

        if (!running) {
            running = true
            thread(name = "openplug-session") {
                Session.run(applicationContext)
                running = false
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf()
            }
            thread(name = "openplug-notification") {
                var shown = content
                while (running) {
                    Thread.sleep(UPDATE_EVERY_MS)
                    val next = Content.now()
                    if (running && next != shown) {
                        shown = next
                        manager.notify(NOTIFICATION_ID, notification(next))
                    }
                }
            }
        }
        // Restart after the system kills the process; the session resumes from Settings.
        return START_STICKY
    }

    @Volatile
    private var running = false

    /** What the notification says; compared to skip updates that change nothing. */
    private data class Content(val title: String, val text: String, val chip: String?) {
        companion object {
            fun now(): Content {
                val journal = Session.journal.value
                val trip = journal.active.takeIf { journal.tripRunning }
                return when (val state = Session.state.value) {
                    is LinkState.Connected -> if (trip != null) {
                        val speed = Session.readings.value.firstOrNull { it.url == SPEED_URL && it.available }
                        val parts = buildList {
                            speed?.let { add("${number(it.value.toDouble(), 0)} km/h") }
                            add(duration(trip.movingS))
                            trip.consumptionLPer100km?.let { add("${number(it, 1)} l/100 km") }
                        }
                        Content("Trip · ${km(trip.distanceKm)}", parts.joinToString(" · "), km(trip.distanceKm))
                    } else {
                        Content("Connected to the car", "A trip starts when the car moves", null)
                    }
                    is LinkState.Waiting, is LinkState.Connecting ->
                        Content("Waiting for the car", "Connects by itself when the ignition is on", null)
                    is LinkState.Failed -> Content("OpenPlug stopped", state.message, null)
                    LinkState.Idle -> Content("OpenPlug", "Not connected", null)
                }
            }
        }
    }

    private fun notification(content: Content): Notification {
        val builder = Notification.Builder(this, CHANNEL)
            .setContentTitle(content.title)
            .setContentText(content.text)
            .setSmallIcon(R.drawable.ic_car)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setContentIntent(
                PendingIntent.getActivity(
                    this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
                ),
            )
        // Android 16+: a trip in progress asks to be shown as a Live Update, with the
        // distance in the status-bar chip. (The request is set as an extra, whose key is
        // a compile-time constant, so no method newer than API 36.0 is called.)
        if (Build.VERSION.SDK_INT >= 36 && content.chip != null) {
            builder.setShortCriticalText(content.chip)
            builder.addExtras(android.os.Bundle().apply { putBoolean(Notification.EXTRA_REQUEST_PROMOTED_ONGOING, true) })
        }
        return builder.build()
    }

    private companion object {
        const val CHANNEL = "session"
        const val NOTIFICATION_ID = 1
        const val UPDATE_EVERY_MS = 3_000L
        const val SPEED_URL = 0x020000u
    }
}
