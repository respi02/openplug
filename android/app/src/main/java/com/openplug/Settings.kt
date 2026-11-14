package com.openplug

import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothManager
import android.content.Context
import android.content.SharedPreferences
import android.os.PowerManager
import com.openplug.transport.BluetoothSppTransport
import com.openplug.transport.DongleTransport
import com.openplug.transport.TcpTransport

/** What a session connects to and how. Kept so a session can resume without the UI. */
data class SessionSettings(val targetId: String, val pin: String, val record: Boolean) {
    val isFake: Boolean get() = targetId == Settings.FAKE_ID
}

/** The app's stored choices (plain preferences; the PIN moves to the Keystore later). */
object Settings {
    /** The debug-only fake dongle's target id (it has a trip log of its own). */
    const val FAKE_ID = "tcp"

    private const val PREFS = "openplug"
    private const val KEY_TARGET = "target"
    private const val KEY_PIN = "pin"
    private const val KEY_RECORD = "record"

    /** True from Connect until the user stops the session (or the PIN is rejected). */
    private const val KEY_WANTED = "session_wanted"

    fun prefs(context: Context): SharedPreferences =
        context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    fun target(context: Context): String? = prefs(context).getString(KEY_TARGET, null)
    fun pin(context: Context): String = prefs(context).getString(KEY_PIN, "") ?: ""

    /** On by default in debug builds, which are the ones taken on test drives. */
    fun record(context: Context): Boolean = prefs(context).getBoolean(KEY_RECORD, BuildConfig.DEBUG)

    fun setRecord(context: Context, on: Boolean) = prefs(context).edit().putBoolean(KEY_RECORD, on).apply()

    fun save(context: Context, s: SessionSettings) {
        prefs(context).edit()
            .putString(KEY_TARGET, s.targetId)
            .putString(KEY_PIN, s.pin)
            .putBoolean(KEY_RECORD, s.record)
            .apply()
    }

    /** The settings of the last session, if complete. */
    fun saved(context: Context): SessionSettings? {
        val target = target(context) ?: return null
        val pin = pin(context).ifEmpty { return null }
        return SessionSettings(target, pin, record(context))
    }

    fun isSessionWanted(context: Context): Boolean = prefs(context).getBoolean(KEY_WANTED, false)

    fun setSessionWanted(context: Context, wanted: Boolean) {
        // commit: this must survive the process being killed right after
        prefs(context).edit().putBoolean(KEY_WANTED, wanted).commit()
    }

    /** Whether the user let the app run unrestricted in the background (Doze exemption). */
    fun isBatteryUnrestricted(context: Context): Boolean =
        context.getSystemService(PowerManager::class.java).isIgnoringBatteryOptimizations(context.packageName)

    /** Opens a link to the target, or null if the target is unknown here. */
    fun transportFor(context: Context, targetId: String): (() -> DongleTransport)? {
        if (targetId == FAKE_ID) {
            return if (BuildConfig.DEBUG) ({ TcpTransport("127.0.0.1", 4455) }) else null
        }
        if (!BluetoothAdapter.checkBluetoothAddress(targetId)) return null
        val adapter = context.getSystemService(BluetoothManager::class.java)?.adapter ?: return null
        val device = adapter.getRemoteDevice(targetId)
        return { BluetoothSppTransport(device) }
    }
}
