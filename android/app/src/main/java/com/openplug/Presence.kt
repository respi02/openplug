package com.openplug

import android.Manifest
import android.app.PendingIntent
import android.bluetooth.BluetoothManager
import android.bluetooth.le.ScanFilter
import android.bluetooth.le.ScanSettings
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.ParcelUuid
import android.util.Log

private const val TAG = "OpenPlug"

/**
 * Notices the dongle coming into reach while the session waits for it.
 *
 * The dongle advertises over BLE whenever it is powered and no phone holds it (it
 * stops once a Classic client is connected). A filtered scan handed to the system
 * with a [PendingIntent] keeps running while the phone sleeps — and even if our
 * process is gone — and wakes [PresenceReceiver] on a match, which then connects
 * over Classic SPP right away instead of at the next timed retry (timed retries stall
 * while the CPU sleeps).
 *
 * Needs BLUETOOTH_SCAN (Android 12+); on older versions, or without the permission,
 * the session falls back to timed retries only.
 */
object Presence {
    /** The dongle's vendor GATT service, which it advertises. */
    private val SERVICE: ParcelUuid = ParcelUuid.fromString("732DB477-BFD6-4694-9378-86C246F35DEF")

    /** Whether this process started the scan (the system keeps it across process death). */
    @Volatile
    private var scanning = false

    fun canScan(context: Context): Boolean =
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S &&
            context.checkSelfPermission(Manifest.permission.BLUETOOTH_SCAN) == PackageManager.PERMISSION_GRANTED

    private fun intent(context: Context): PendingIntent = PendingIntent.getBroadcast(
        context,
        0,
        Intent(context, PresenceReceiver::class.java),
        // mutable: the system adds the scan results as extras
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE,
    )

    @Suppress("MissingPermission") // checked in canScan()
    fun start(context: Context) {
        if (scanning || !canScan(context)) return
        val scanner = context.getSystemService(BluetoothManager::class.java)?.adapter?.bluetoothLeScanner ?: return
        val filters = listOf(ScanFilter.Builder().setServiceUuid(SERVICE).build())
        val settings = ScanSettings.Builder().setScanMode(ScanSettings.SCAN_MODE_LOW_POWER).build()
        val rc = runCatching { scanner.startScan(filters, settings, intent(context)) }
        scanning = rc.getOrNull() == 0
        Log.i(TAG, "presence scan started: $rc")
    }

    @Suppress("MissingPermission") // checked in canScan()
    fun stop(context: Context) {
        scanning = false
        if (!canScan(context)) return
        val scanner = context.getSystemService(BluetoothManager::class.java)?.adapter?.bluetoothLeScanner ?: return
        runCatching { scanner.stopScan(intent(context)) }
    }
}

/** A dongle was seen: connect now, resuming the session if the process was gone. */
class PresenceReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        Log.i(TAG, "dongle advertising nearby")
        if (Session.isActive) {
            Session.kick(context)
        } else if (!Session.resume(context)) {
            Presence.stop(context) // nothing to resume: stop scanning
        }
    }
}

/** After a reboot or an app update, picks the session up again if it was on. */
class RestartReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (intent.action) {
            Intent.ACTION_BOOT_COMPLETED, Intent.ACTION_MY_PACKAGE_REPLACED -> {
                Log.i(TAG, "${intent.action}: resuming = ${Session.resume(context)}")
            }
        }
    }
}
