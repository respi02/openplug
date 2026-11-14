package com.openplug

import android.Manifest
import android.annotation.SuppressLint
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothManager
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.text.format.DateUtils
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.openplug.core.Reading
import com.openplug.core.TripRecord
import com.openplug.transport.BluetoothSppTransport
import com.openplug.transport.DongleTransport
import com.openplug.transport.TcpTransport
import java.util.Locale

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        Session.openJournal(applicationContext)
        setContent {
            MaterialTheme(if (isSystemInDarkTheme()) darkColorScheme() else lightColorScheme()) {
                OpenPlugScreen()
            }
        }
    }
}

/** Something the user can connect to: a bonded dongle, or (debug) the fake one. */
private data class Target(val id: String, val label: String, val open: () -> DongleTransport)

private const val PREFS = "openplug"

/** The debug-only fake dongle's target id (it has a trip log of its own). */
private const val FAKE_ID = "tcp"

/** Bluetooth to reach the dongle; notifications to show the session while it runs. */
private fun wantedPermissions(): Array<String> = buildList {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) add(Manifest.permission.BLUETOOTH_CONNECT)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) add(Manifest.permission.POST_NOTIFICATIONS)
}.toTypedArray()

private fun hasBluetoothPermission(context: Context): Boolean =
    Build.VERSION.SDK_INT < Build.VERSION_CODES.S ||
        ContextCompat.checkSelfPermission(context, Manifest.permission.BLUETOOTH_CONNECT) ==
        PackageManager.PERMISSION_GRANTED

@SuppressLint("MissingPermission") // called only when hasBluetoothPermission() is true
private fun targets(context: Context): List<Target> {
    val bonded: Set<BluetoothDevice> =
        context.getSystemService(BluetoothManager::class.java)?.adapter?.bondedDevices.orEmpty()
    val dongles = bonded
        .sortedByDescending { it.name?.startsWith("DP_") == true } // DataPlugs first
        .map { d -> Target(d.address, d.name ?: d.address) { BluetoothSppTransport(d) } }
    val fake = Target(FAKE_ID, "Fake dongle (TCP 127.0.0.1:4455)") { TcpTransport("127.0.0.1", 4455) }
    return if (BuildConfig.DEBUG) dongles + fake else dongles
}

@Composable
fun OpenPlugScreen() {
    var tab by rememberSaveable { mutableIntStateOf(0) }
    Scaffold(
        bottomBar = {
            NavigationBar {
                NavigationBarItem(selected = tab == 0, onClick = { tab = 0 }, icon = {}, label = { Text("Live") })
                NavigationBarItem(selected = tab == 1, onClick = { tab = 1 }, icon = {}, label = { Text("Trips") })
            }
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding).padding(horizontal = 16.dp)) {
            if (tab == 0) LiveTab() else TripsTab()
        }
    }
}

// ---- Live values ----

@Composable
private fun LiveTab() {
    val state by Session.state.collectAsStateWithLifecycle()
    val readings by Session.readings.collectAsStateWithLifecycle()
    val context = LocalContext.current

    Text("OpenPlug", style = MaterialTheme.typography.headlineMedium)
    Spacer(Modifier.height(12.dp))
    when (state) {
        LinkState.Connected -> Live(readings, onDisconnect = Session::stop)
        else -> Setup(
            state,
            onConnect = { target, pin, record ->
                Session.start(context.applicationContext, target.open, pin, record, demo = target.id == FAKE_ID)
            },
            onStop = Session::stop,
        )
    }
}

@Composable
private fun Setup(state: LinkState, onConnect: (Target, String, Boolean) -> Unit, onStop: () -> Unit) {
    val context = LocalContext.current
    val prefs = remember { context.getSharedPreferences(PREFS, Context.MODE_PRIVATE) }
    var granted by remember { mutableStateOf(hasBluetoothPermission(context)) }
    val askPermissions = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {
        granted = hasBluetoothPermission(context)
    }
    var pin by remember { mutableStateOf(prefs.getString("pin", "") ?: "") }
    var selected by remember { mutableStateOf(prefs.getString("target", null)) }
    // on by default in debug builds, which are the ones taken on test drives
    var record by remember { mutableStateOf(prefs.getBoolean("record", BuildConfig.DEBUG)) }
    val busy = state is LinkState.Connecting || state is LinkState.Waiting

    if (!granted) {
        Text("OpenPlug needs the Nearby devices permission to reach the dongle over Bluetooth, and shows a notification while it is connected.")
        Spacer(Modifier.height(8.dp))
        Button(onClick = { askPermissions.launch(wantedPermissions()) }) { Text("Allow") }
        return
    }

    val all = remember(granted) { targets(context) }
    if (all.isEmpty()) {
        Text("No paired Bluetooth devices. Pair the dongle (DP_…) in the system Bluetooth settings first.")
        return
    }

    Text("Dongle", style = MaterialTheme.typography.titleMedium)
    all.forEach { t ->
        Row(
            Modifier.fillMaxWidth().selectable(selected = t.id == selected, enabled = !busy) { selected = t.id },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            RadioButton(selected = t.id == selected, onClick = null, modifier = Modifier.padding(12.dp))
            Text(t.label)
        }
    }
    Spacer(Modifier.height(8.dp))
    OutlinedTextField(
        value = pin,
        onValueChange = { pin = it.trim() },
        label = { Text("PIN from the dongle label") },
        singleLine = true,
        enabled = !busy,
        visualTransformation = PasswordVisualTransformation(),
        // label PINs are alphanumeric (ours is 8 capital letters)
        keyboardOptions = KeyboardOptions(
            capitalization = KeyboardCapitalization.Characters,
            keyboardType = KeyboardType.Password,
        ),
        modifier = Modifier.fillMaxWidth(),
    )
    Spacer(Modifier.height(8.dp))
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text("Record values")
            Text(
                "Saves what the dongle sends (no position, no VIN) to a file, to replay the drive later.",
                style = MaterialTheme.typography.bodySmall,
            )
        }
        Switch(checked = record, enabled = !busy, onCheckedChange = {
            record = it
            prefs.edit().putBoolean("record", it).apply()
        })
    }
    Spacer(Modifier.height(12.dp))
    val target = all.firstOrNull { it.id == selected }
    if (busy) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            OutlinedButton(onClick = onStop) { Text("Stop") }
            CircularProgressIndicator(Modifier.padding(start = 16.dp))
        }
        Spacer(Modifier.height(12.dp))
        Text(if (state is LinkState.Connecting) state.what else (state as LinkState.Waiting).what)
        Spacer(Modifier.height(8.dp))
        Text(
            "OpenPlug keeps trying until the dongle answers — it only does when the ignition is on.",
            style = MaterialTheme.typography.bodySmall,
        )
    } else {
        Button(
            enabled = target != null && pin.isNotEmpty(),
            onClick = {
                prefs.edit().putString("pin", pin).putString("target", selected).apply()
                askPermissions.launch(wantedPermissions()) // notifications, if not asked yet
                onConnect(target!!, pin, record)
            },
        ) { Text("Connect") }
    }
    if (state is LinkState.Failed) {
        Spacer(Modifier.height(12.dp))
        Text(state.message, color = MaterialTheme.colorScheme.error)
    }
}

@Composable
private fun Live(readings: List<Reading>, onDisconnect: () -> Unit) {
    val recording by Session.recording.collectAsStateWithLifecycle()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text("Connected", Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
        OutlinedButton(onClick = onDisconnect) { Text("Disconnect") }
    }
    recording?.let {
        Text("Recording to ${it.substringAfterLast('/')}", style = MaterialTheme.typography.bodySmall)
    }
    Spacer(Modifier.height(8.dp))
    if (readings.isEmpty()) {
        Text("Waiting for the first values…")
    }
    LazyColumn {
        items(readings) { r -> // no keys: a keyed list would scroll away from rows inserted above
            ListItem(
                headlineContent = { Text(r.name ?: String.format(Locale.ROOT, "0x%06x", r.url.toInt())) },
                trailingContent = { Text(formatValue(r), style = MaterialTheme.typography.titleMedium) },
            )
            HorizontalDivider()
        }
    }
}

private fun formatValue(r: Reading): String {
    if (!r.available) return "—"
    val number = String.format(Locale.getDefault(), "%.${r.precision.toInt()}f", r.value)
    return if (r.unit != null) "$number ${r.unit}" else number
}

// ---- Trips ----

@Composable
private fun TripsTab() {
    val j by Session.journal.collectAsStateWithLifecycle()
    val problem by Session.journalProblem.collectAsStateWithLifecycle()

    Text("Trips", style = MaterialTheme.typography.headlineMedium)
    Spacer(Modifier.height(12.dp))
    problem?.let {
        Text("Trips are not being recorded: $it", color = MaterialTheme.colorScheme.error)
        return
    }
    if (j.active == null && j.trips.isEmpty() && j.refuels.isEmpty()) {
        Text("No trips yet. A trip starts when the car moves while OpenPlug is connected, and ends 15 minutes after the connection drops.")
        return
    }
    LazyColumn {
        j.active?.let { trip ->
            item { SectionTitle("Current trip") }
            item { TripRow(trip, running = true) }
        }
        if (j.trips.isNotEmpty()) {
            item { SectionTitle("All trips") }
            item {
                val t = j.totals
                val fuel = if (t.fuelUsedL > 0) " · ${litres(t.fuelUsedL)}" else ""
                ListItem(
                    headlineContent = { Text("${t.trips} trips · ${km(t.distanceKm)}") },
                    supportingContent = { Text("${duration(t.movingS)} driving$fuel") },
                )
            }
            item { SectionTitle("History") }
            items(j.trips) { TripRow(it, running = false) }
        }
        if (j.refuels.isNotEmpty()) {
            item { SectionTitle("Refuels") }
            items(j.refuels) { r ->
                val at = r.odometerKm?.let { " · at ${String.format(Locale.getDefault(), "%.0f", it)} km" } ?: ""
                ListItem(
                    headlineContent = { Text(date(r.timeMs)) },
                    supportingContent = { if (at.isNotEmpty()) Text(at.removePrefix(" · ")) },
                    trailingContent = { Text("+${litres(r.addedL)}", style = MaterialTheme.typography.titleMedium) },
                )
                HorizontalDivider()
            }
        }
    }
}

@Composable
private fun SectionTitle(text: String) {
    Text(text, Modifier.padding(top = 16.dp, bottom = 4.dp), style = MaterialTheme.typography.titleSmall)
}

@Composable
private fun TripRow(trip: TripRecord, running: Boolean) {
    val parts = buildList {
        add(duration(trip.movingS) + if (running) " so far" else "")
        if (trip.movingS > 0uL) add(String.format(Locale.getDefault(), "avg %.0f km/h", trip.avgSpeedKmh))
        add(String.format(Locale.getDefault(), "max %.0f km/h", trip.maxSpeedKmh))
        val consumption = trip.consumptionLPer100km
        val fuel = trip.fuelUsedL
        if (consumption != null) {
            add(String.format(Locale.getDefault(), "%.1f l/100 km", consumption))
        } else if (fuel != null && fuel > 0) {
            add(litres(fuel))
        }
    }
    ListItem(
        headlineContent = { Text(date(trip.startMs)) },
        supportingContent = { Text(parts.joinToString(" · ")) },
        trailingContent = { Text(km(trip.distanceKm), style = MaterialTheme.typography.titleMedium) },
    )
    HorizontalDivider()
}

private fun km(v: Double) = String.format(Locale.getDefault(), if (v < 100) "%.1f km" else "%.0f km", v)

private fun litres(v: Double) = String.format(Locale.getDefault(), "%.1f l", v)

private fun duration(seconds: ULong): String {
    val m = (seconds.toLong() + 30) / 60
    return if (m < 60) "$m min" else "${m / 60} h ${m % 60} min"
}

@Composable
private fun date(ms: ULong): String = DateUtils.formatDateTime(
    LocalContext.current,
    ms.toLong(),
    DateUtils.FORMAT_SHOW_DATE or DateUtils.FORMAT_SHOW_TIME or DateUtils.FORMAT_ABBREV_MONTH,
)
