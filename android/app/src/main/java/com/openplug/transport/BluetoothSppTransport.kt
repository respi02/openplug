package com.openplug.transport

import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothSocket
import com.openplug.core.ByteTransport
import com.openplug.core.TransportException
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.util.UUID

/**
 * Transport over Bluetooth Classic RFCOMM/SPP, feeding bytes to the shared Rust
 * core (which implements TLS-SRP + the dongle protocol on top of these bytes).
 *
 * Implements the UniFFI-generated `ByteTransport` interface (from `core/src/ffi.rs`).
 * On SPP the TLS records travel directly on the serial channel — no extra framing.
 *
 * NOTE: scaffold — written against the generated bindings but not yet compiled.
 */
class BluetoothSppTransport(device: BluetoothDevice) : ByteTransport {

    // Standard Serial Port Profile UUID.
    private val sppUuid: UUID = UUID.fromString("00001101-0000-1000-8000-00805F9B34FB")

    private val socket: BluetoothSocket =
        device.createRfcommSocketToServiceRecord(sppUuid)
    private lateinit var input: InputStream
    private lateinit var output: OutputStream

    /** Blocking; call off the main thread. Requires BLUETOOTH_CONNECT. */
    fun open() {
        socket.connect()
        input = socket.inputStream
        output = socket.outputStream
    }

    override fun send(data: ByteArray) {
        try {
            output.write(data)
            output.flush()
        } catch (e: IOException) {
            throw TransportException.Io(e.message ?: "write failed")
        }
    }

    /** Blocks for at least one byte; an empty array means the connection is closed. */
    override fun recv(maxLen: UInt): ByteArray {
        val buf = ByteArray(maxLen.toInt())
        val n = try {
            input.read(buf)
        } catch (e: IOException) {
            // Android reports a remote close as an IOException rather than -1.
            -1
        }
        return if (n <= 0) ByteArray(0) else buf.copyOf(n)
    }

    fun close() {
        runCatching { socket.close() }
    }
}
