package com.openplug

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.openplug.core.Dongle
import com.openplug.core.DongleException
import com.openplug.core.Reading
import com.openplug.transport.DongleTransport
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

sealed interface LinkState {
    data object Idle : LinkState
    data object Connecting : LinkState
    data object Connected : LinkState
    data class Failed(val message: String) : LinkState
}

/** Owns the connection: opens the transport, authenticates, and pumps readings. */
class DongleViewModel : ViewModel() {

    private val _state = MutableStateFlow<LinkState>(LinkState.Idle)
    val state: StateFlow<LinkState> = _state

    private val _readings = MutableStateFlow<List<Reading>>(emptyList())
    val readings: StateFlow<List<Reading>> = _readings

    @Volatile
    private var transport: DongleTransport? = null

    @Volatile
    private var userClosed = false

    fun connect(transport: DongleTransport, pin: String) {
        if (_state.value is LinkState.Connecting || _state.value is LinkState.Connected) return
        this.transport = transport
        userClosed = false
        _readings.value = emptyList()
        _state.value = LinkState.Connecting

        // Every core call blocks, so the whole session lives on an IO thread.
        viewModelScope.launch(Dispatchers.IO) {
            val end = try {
                transport.open()
                Dongle.connect(transport, pin).use { dongle ->
                    dongle.subscribeDefaults()
                    _state.value = LinkState.Connected
                    val latest = LinkedHashMap<UInt, Reading>()
                    while (true) {
                        for (r in dongle.nextReadings()) latest[r.url] = r
                        _readings.value = latest.values.sortedWith(
                            compareBy<Reading> { it.name == null }.thenBy { it.url },
                        )
                    }
                    @Suppress("UNREACHABLE_CODE")
                    LinkState.Idle
                }
            } catch (e: DongleException.WrongPin) {
                LinkState.Failed("The dongle rejected the PIN.")
            } catch (e: DongleException.Closed) {
                LinkState.Failed("The dongle closed the connection.")
            } catch (e: Exception) {
                LinkState.Failed(e.message ?: e.javaClass.simpleName)
            } finally {
                transport.close()
            }
            _state.value = if (userClosed) LinkState.Idle else end
        }
    }

    /** Closing the transport unblocks the reader, which then ends the session. */
    fun disconnect() {
        userClosed = true
        transport?.close()
    }

    override fun onCleared() = disconnect()
}
