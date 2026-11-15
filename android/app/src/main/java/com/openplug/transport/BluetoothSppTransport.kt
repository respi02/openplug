package com.openplug.transport

import android.annotation.SuppressLint
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothSocket
import java.io.IOException
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap

/**
 * Bluetooth Classic RFCOMM/SPP to the dongle. The shared Rust core runs TLS-SRP and
 * the dongle protocol directly on these bytes — on SPP there is no extra framing.
 *
 * Like the original app, it connects with an insecure socket first, which needs no
 * bond (the dongle is authenticated by TLS-SRP with the label PIN anyway), and falls
 * back to a secure one. The kind that worked is tried first next time.
 */
class BluetoothSppTransport(private val device: BluetoothDevice) : StreamTransport() {

    @Volatile
    private var socket: BluetoothSocket? = null

    @Volatile
    private var closed = false

    @SuppressLint("MissingPermission") // the UI only offers devices once BLUETOOTH_CONNECT is granted
    override fun open() {
        val order = if (secureWorked[device.address] == true) listOf(true, false) else listOf(false, true)
        var failure: IOException? = null
        for (secure in order) {
            if (closed) break
            val s = if (secure) {
                device.createRfcommSocketToServiceRecord(SPP_UUID)
            } else {
                device.createInsecureRfcommSocketToServiceRecord(SPP_UUID)
            }
            socket = s
            try {
                s.connect()
            } catch (e: IOException) {
                failure = e
                runCatching { s.close() }
                continue
            }
            secureWorked[device.address] = secure
            input = s.inputStream
            output = s.outputStream
            return
        }
        throw failure ?: IOException("closed")
    }

    override fun close() {
        closed = true
        runCatching { socket?.close() }
    }

    private companion object {
        /** Standard Serial Port Profile UUID. */
        val SPP_UUID: UUID = UUID.fromString("00001101-0000-1000-8000-00805F9B34FB")

        /** Per dongle address: whether the secure socket was the one that connected. */
        val secureWorked = ConcurrentHashMap<String, Boolean>()
    }
}
