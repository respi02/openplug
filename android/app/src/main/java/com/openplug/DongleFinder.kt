package com.openplug

import android.Manifest
import android.annotation.SuppressLint
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothManager
import android.bluetooth.le.ScanCallback
import android.bluetooth.le.ScanFilter
import android.bluetooth.le.ScanResult
import android.bluetooth.le.ScanSettings
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.ParcelUuid
import android.util.Log
import androidx.core.content.ContextCompat

/**
 * Finds dongles nearby and pairs with them, the way the original app did: no pairing
 * in the system Bluetooth settings (the dongle makers say not to).
 *
 * Two searches run together. Classic discovery finds the dongle by its name, which
 * starts with `DP_` (`DP_2_0…`, `DP_2_1…`, `DP_2_2…` for the hardware revisions); a
 * BLE scan finds it by the vendor service it advertises, whatever its name. The dongle
 * is dual-mode with one address, so either result can be connected over Classic SPP.
 */
object DongleFinder {
    private const val TAG = "OpenPlug"
    private const val SEARCH_MS = 15_000L
    private val SERVICE: ParcelUuid = ParcelUuid.fromString("732DB477-BFD6-4694-9378-86C246F35DEF")

    /** Whether a name is a dongle's. */
    fun isDongleName(name: String?): Boolean = name?.startsWith("DP_") == true

    /** The permissions a search needs: scanning (12+), or location for discovery before that. */
    fun permissions(): Array<String> = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        arrayOf(Manifest.permission.BLUETOOTH_SCAN, Manifest.permission.BLUETOOTH_CONNECT)
    } else {
        arrayOf(Manifest.permission.ACCESS_FINE_LOCATION)
    }

    fun canSearch(context: Context): Boolean = permissions().all {
        ContextCompat.checkSelfPermission(context, it) == PackageManager.PERMISSION_GRANTED
    }

    fun adapter(context: Context): BluetoothAdapter? = context.getSystemService(BluetoothManager::class.java)?.adapter

    /**
     * Searches for about 15 s. [onFound] gets each dongle once (address, name), on the
     * main thread; [onDone] when the search is over. Returns a function that stops it.
     */
    @SuppressLint("MissingPermission") // canSearch() is checked by the caller
    fun search(context: Context, onFound: (String, String?) -> Unit, onDone: () -> Unit): () -> Unit {
        val app = context.applicationContext
        val adapter = adapter(app)
        if (adapter == null || !adapter.isEnabled || !canSearch(app)) {
            onDone()
            return {}
        }
        val main = Handler(Looper.getMainLooper())
        val seen = HashSet<String>()
        fun found(device: BluetoothDevice, name: String?) {
            main.post {
                if (seen.add(device.address)) {
                    Log.i(TAG, "found dongle ${device.address} ($name)")
                    onFound(device.address, name)
                }
            }
        }

        val receiver = object : BroadcastReceiver() {
            override fun onReceive(c: Context, intent: Intent) {
                if (intent.action != BluetoothDevice.ACTION_FOUND) return
                @Suppress("DEPRECATION")
                val device = intent.getParcelableExtra<BluetoothDevice>(BluetoothDevice.EXTRA_DEVICE) ?: return
                val name = intent.getStringExtra(BluetoothDevice.EXTRA_NAME) ?: device.name
                if (isDongleName(name)) found(device, name)
            }
        }
        ContextCompat.registerReceiver(
            app, receiver, IntentFilter(BluetoothDevice.ACTION_FOUND), ContextCompat.RECEIVER_NOT_EXPORTED,
        )
        val ble = object : ScanCallback() {
            override fun onScanResult(callbackType: Int, result: ScanResult) {
                found(result.device, result.scanRecord?.deviceName ?: result.device.name)
            }
        }
        val scanner = adapter.bluetoothLeScanner
        runCatching {
            scanner?.startScan(
                listOf(ScanFilter.Builder().setServiceUuid(SERVICE).build()),
                ScanSettings.Builder().setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY).build(),
                ble,
            )
        }.onFailure { Log.w(TAG, "BLE search: $it") }
        val discovering = runCatching { adapter.startDiscovery() }.getOrDefault(false)
        Log.i(TAG, "searching for dongles (discovery started: $discovering)")

        var stopped = false
        val stop = {
            if (!stopped) {
                stopped = true
                main.removeCallbacksAndMessages(null)
                runCatching { adapter.cancelDiscovery() }
                runCatching { scanner?.stopScan(ble) }
                runCatching { app.unregisterReceiver(receiver) }
                onDone()
            }
        }
        main.postDelayed({ stop() }, SEARCH_MS)
        return stop
    }

    /**
     * Pairs with a dongle that is not paired yet, as the original app did before its
     * first connection; the system may ask the user to confirm. Calls [onDone] on the
     * main thread once the pairing is over, whether it worked or not (the connection
     * is tried either way: it needs no pairing). Returns a function that stops waiting.
     */
    @SuppressLint("MissingPermission") // BLUETOOTH_CONNECT is granted before targets are shown
    fun pair(context: Context, address: String, onDone: (Boolean) -> Unit): () -> Unit {
        val app = context.applicationContext
        val device = adapter(app)?.takeIf { BluetoothAdapter.checkBluetoothAddress(address) }?.getRemoteDevice(address)
        if (device == null || device.bondState == BluetoothDevice.BOND_BONDED) {
            onDone(device != null)
            return {}
        }
        val main = Handler(Looper.getMainLooper())
        var finished = false
        lateinit var receiver: BroadcastReceiver
        fun finish(bonded: Boolean) {
            if (finished) return
            finished = true
            main.removeCallbacksAndMessages(null)
            runCatching { app.unregisterReceiver(receiver) }
            Log.i(TAG, "pairing with $address: bonded=$bonded")
            onDone(bonded)
        }
        receiver = object : BroadcastReceiver() {
            override fun onReceive(c: Context, intent: Intent) {
                @Suppress("DEPRECATION")
                val d = intent.getParcelableExtra<BluetoothDevice>(BluetoothDevice.EXTRA_DEVICE) ?: return
                if (d.address != address) return
                when (intent.getIntExtra(BluetoothDevice.EXTRA_BOND_STATE, BluetoothDevice.BOND_NONE)) {
                    BluetoothDevice.BOND_BONDED -> finish(true)
                    BluetoothDevice.BOND_NONE -> finish(false)
                }
            }
        }
        ContextCompat.registerReceiver(
            app, receiver, IntentFilter(BluetoothDevice.ACTION_BOND_STATE_CHANGED), ContextCompat.RECEIVER_NOT_EXPORTED,
        )
        runCatching { adapter(app)?.cancelDiscovery() } // discovery slows pairing down
        if (!device.createBond()) {
            finish(false)
        } else {
            main.postDelayed({ finish(device.bondState == BluetoothDevice.BOND_BONDED) }, 60_000)
        }
        return { finish(device.bondState == BluetoothDevice.BOND_BONDED) }
    }
}
