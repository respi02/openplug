package com.openplug.transport

import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothSocket
import java.io.InputStream
import java.io.OutputStream
import java.util.UUID

/**
 * Transport over Bluetooth Classic RFCOMM/SPP, feeding bytes to the shared Rust
 * core (which implements TLS-SRP + the dongle protocol on top of these bytes).
 *
 * Implements the UniFFI-generated `Transport` callback interface (see
 * android/uniffi/openplug.udl). On Android the dongle uses the Classic/SPP side;
 * iOS uses the BLE side of the same dual-mode dongle (see ios/).
 *
 * NOTE: scaffold — the UniFFI `Transport` base type is generated at build time.
 */
class BluetoothSppTransport(device: BluetoothDevice) /* : Transport */ {

    // Standard Serial Port Profile UUID.
    private val sppUuid: UUID = UUID.fromString("00001101-0000-1000-8000-00805F9B34FB")

    private val socket: BluetoothSocket =
        device.createRfcommSocketToServiceRecord(sppUuid)
    private lateinit var input: InputStream
    private lateinit var output: OutputStream

    fun open() {
        socket.connect()               // pairing uses the PIN on the dongle label
        input = socket.inputStream
        output = socket.outputStream
    }

    /** UniFFI: send all bytes. */
    fun send(data: ByteArray) {
        output.write(data)
        output.flush()
    }

    /** UniFFI: read up to maxLen bytes (blocking; returns >0 or empty on close). */
    fun recv(maxLen: Int): ByteArray {
        val buf = ByteArray(maxLen)
        val n = input.read(buf)
        return if (n <= 0) ByteArray(0) else buf.copyOf(n)
    }

    fun close() {
        runCatching { socket.close() }
    }
}
