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
 * recorded from start to end. Shows an ongoing notification while it runs. Sticky: if
 * the system kills the process, the service comes back and resumes the session.
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
        val notification = Notification.Builder(this, CHANNEL)
            .setContentTitle("OpenPlug")
            .setContentText("Recording trips while the car is in reach")
            .setSmallIcon(android.R.drawable.stat_sys_data_bluetooth)
            .setOngoing(true)
            .setContentIntent(
                PendingIntent.getActivity(
                    this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
                ),
            )
            .build()
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
            } else {
                startForeground(NOTIFICATION_ID, notification)
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
        }
        // Restart after the system kills the process; the session resumes from Settings.
        return START_STICKY
    }

    @Volatile
    private var running = false

    private companion object {
        const val CHANNEL = "session"
        const val NOTIFICATION_ID = 1
    }
}
