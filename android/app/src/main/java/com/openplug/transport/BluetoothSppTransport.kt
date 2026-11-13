package com.openplug.transport

import android.annotation.SuppressLint
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothSocket
import java.util.UUID

/**
 * Bluetooth Classic RFCOMM/SPP to a bonded dongle. The shared Rust core runs
 * TLS-SRP and the dongle protocol directly on these bytes — on SPP there is no
 * extra framing.
 */
class BluetoothSppTransport(private val device: BluetoothDevice) : StreamTransport() {

    @Volatile
    private var socket: BluetoothSocket? = null

    @SuppressLint("MissingPermission") // the UI only offers devices once BLUETOOTH_CONNECT is granted
    override fun open() {
        val s = device.createRfcommSocketToServiceRecord(SPP_UUID)
        socket = s
        s.connect()
        input = s.inputStream
        output = s.outputStream
    }

    override fun close() {
        runCatching { socket?.close() }
    }

    private companion object {
        /** Standard Serial Port Profile UUID. */
        val SPP_UUID: UUID = UUID.fromString("00001101-0000-1000-8000-00805F9B34FB")
    }
}
