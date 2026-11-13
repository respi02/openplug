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
import kotlin.concurrent.thread

/**
 * Keeps the dongle session alive while the app is in the background, so a drive is
 * recorded from start to end. Shows an ongoing notification while it runs.
 */
class DongleService : Service() {

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, "Dongle connection", NotificationManager.IMPORTANCE_LOW),
        )
        val notification = Notification.Builder(this, CHANNEL)
            .setContentTitle("OpenPlug")
            .setContentText("Connected to the car — recording trips")
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
        } catch (e: SecurityException) {
            // Without the Bluetooth permission the system refuses this service type.
            Session.stop()
            stopSelf()
            return START_NOT_STICKY
        }

        thread(name = "openplug-session") {
            Session.openJournal(applicationContext)
            Session.run()
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
        }
        // If the system kills the process, the user reconnects; nothing to restore.
        return START_NOT_STICKY
    }

    private companion object {
        const val CHANNEL = "session"
        const val NOTIFICATION_ID = 1
    }
}
