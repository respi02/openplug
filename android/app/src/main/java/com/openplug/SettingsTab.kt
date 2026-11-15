package com.openplug

import android.Manifest
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import android.bluetooth.BluetoothAdapter
import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LifecycleResumeEffect
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.openplug.core.CarSettings
import com.openplug.core.Reading
import java.util.Locale

/** Something the user can connect to: a dongle, or (debug) the fake one. [detail] says how it is known. */
private data class Target(val id: String, val label: String, val detail: String? = null)

/**
 * Bluetooth to reach the dongle (and to notice it advertising nearby, which lets a
 * waiting session reconnect while the phone sleeps); notifications to show the
 * session while it runs.
 */
private fun wantedPermissions(): Array<String> = buildList {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        add(Manifest.permission.BLUETOOTH_CONNECT)
        add(Manifest.permission.BLUETOOTH_SCAN)
    }
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) add(Manifest.permission.POST_NOTIFICATIONS)
}.toTypedArray()

private fun hasBluetoothPermission(context: Context): Boolean =
    Build.VERSION.SDK_INT < Build.VERSION_CODES.S ||
        ContextCompat.checkSelfPermission(context, Manifest.permission.BLUETOOTH_CONNECT) ==
        PackageManager.PERMISSION_GRANTED

@SuppressLint("MissingPermission") // called only when hasBluetoothPermission() is true
private fun targets(context: Context, found: List<Target>): List<Target> {
    val bonded = DongleFinder.adapter(context)?.bondedDevices.orEmpty()
        .filter { DongleFinder.isDongleName(it.name) }
        .map { d -> Target(d.address, d.name ?: d.address, "paired") }
    val list = (bonded + found.filter { f -> bonded.none { it.id == f.id } }).toMutableList()
    // the one chosen before, also when it is out of reach and not paired
    val saved = Settings.target(context)
    if (saved != null && saved != Settings.FAKE_ID && list.none { it.id == saved }) {
        list += Target(saved, Settings.targetName(context) ?: saved, "chosen before")
    }
    if (BuildConfig.DEBUG) list += Target(Settings.FAKE_ID, "Fake dongle (TCP 127.0.0.1:4455)")
    return list
}

@Composable
fun SettingsTab() {
    // after "Erase all data", start over with what is stored now (nothing)
    val erasures by Session.erasures.collectAsStateWithLifecycle()
    key(erasures) { SettingsContent() }
}

@Composable
private fun SettingsContent() {
    var showSignals by rememberSaveable { mutableStateOf(false) }
    if (showSignals) {
        BackHandler { showSignals = false }
        Signals(onBack = { showSignals = false })
        return
    }
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 20.dp, vertical = 16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        ScreenTitle("Settings")
        Section("DONGLE")
        Panel { DongleSection() }
        Section("CAR")
        Panel { CarSection() }
        Text(
            "The tank size turns the litres the car reports into a fuel gauge. With the service interval of your car's service plan, OpenPlug counts down to the next service from the car's own \"since the last service\" counters, and reminds you 1000 km or 30 days before.",
            Modifier.padding(horizontal = 4.dp),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Section("RECORDING")
        Panel { RecordingSection() }
        Section("DATA")
        Panel { DataSection(onSignals = { showSignals = true }) }
        Text(
            "OpenPlug ${BuildConfig.VERSION_NAME} · open source (MIT). Talks to your dongle locally over Bluetooth — no account, no server. Not affiliated with the vehicle or dongle maker.",
            Modifier.padding(top = 8.dp, start = 4.dp, end = 4.dp),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun Section(title: String) {
    Caption(title, Modifier.padding(top = 10.dp))
}

@Composable
private fun Divider() {
    HorizontalDivider(Modifier.padding(vertical = 12.dp), color = MaterialTheme.colorScheme.outlineVariant)
}

// ---- dongle: choose, PIN, connect / stop ----

@Composable
private fun DongleSection() {
    val context = LocalContext.current
    val state by Session.state.collectAsStateWithLifecycle()
    var granted by remember { mutableStateOf(hasBluetoothPermission(context)) }
    val askPermissions = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {
        granted = hasBluetoothPermission(context)
    }
    if (!granted) {
        Text("OpenPlug needs the Nearby devices permission to find the dongle and reach it over Bluetooth, and shows a notification while it is connected.")
        Spacer(Modifier.height(8.dp))
        Button(onClick = { askPermissions.launch(wantedPermissions()) }) { Text("Allow") }
        return
    }

    val found = remember { mutableStateListOf<Target>() }
    var searching by remember { mutableStateOf(false) }
    var stopSearch by remember { mutableStateOf<(() -> Unit)?>(null) }
    var pairing by remember { mutableStateOf(false) }
    var refresh by remember { mutableIntStateOf(0) }
    var bluetoothOn by remember { mutableStateOf(DongleFinder.adapter(context)?.isEnabled == true) }
    LifecycleResumeEffect(Unit) {
        bluetoothOn = DongleFinder.adapter(context)?.isEnabled == true
        refresh++
        onPauseOrDispose { stopSearch?.invoke() }
    }
    fun search() {
        if (searching || !bluetoothOn) return
        searching = true
        stopSearch = DongleFinder.search(
            context,
            onFound = { address, name ->
                if (found.none { it.id == address }) found += Target(address, name ?: address, "nearby")
            },
            onDone = {
                searching = false
                stopSearch = null
                refresh++
            },
        )
    }
    val askSearch = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {
        if (DongleFinder.canSearch(context)) search()
    }
    fun find() = if (DongleFinder.canSearch(context)) search() else askSearch.launch(DongleFinder.permissions())
    val enableBluetooth = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        bluetoothOn = DongleFinder.adapter(context)?.isEnabled == true
        if (bluetoothOn) find()
    }

    val all = remember(refresh, found.size) { targets(context, found) }
    var pin by remember { mutableStateOf(Settings.pin(context)) }
    var selected by remember { mutableStateOf(Settings.target(context)) }
    val active = state !is LinkState.Idle && state !is LinkState.Failed

    // nothing known yet: look right away
    LaunchedEffect(bluetoothOn) {
        if (bluetoothOn && all.none { it.id != Settings.FAKE_ID } && DongleFinder.canSearch(context)) search()
    }

    if (active) {
        // a running session: what it is doing, and the way out
        val name = all.firstOrNull { it.id == selected }?.label ?: "Dongle"
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(name, fontWeight = FontWeight.SemiBold)
                Text(
                    when (val s = state) {
                        is LinkState.Connected -> "Connected"
                        is LinkState.Connecting -> s.what
                        is LinkState.Waiting -> s.what
                        else -> ""
                    },
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            if (state !is LinkState.Connected) CircularProgressIndicator(Modifier.height(24.dp).padding(start = 8.dp), strokeWidth = 2.dp)
        }
        Divider()
        TextButton(onClick = { Session.stop(context) }, contentPadding = PaddingValues(0.dp)) {
            Text("Disconnect and stop", color = MaterialTheme.colorScheme.error)
        }
        return
    }

    if (!bluetoothOn) {
        Text("Bluetooth is off.")
        Spacer(Modifier.height(8.dp))
        Button(onClick = { enableBluetooth.launch(Intent(BluetoothAdapter.ACTION_REQUEST_ENABLE)) }) { Text("Turn on Bluetooth") }
        Spacer(Modifier.height(12.dp))
    }
    all.forEach { t ->
        Row(
            Modifier.fillMaxWidth().heightIn(min = 44.dp).selectable(selected = t.id == selected) { selected = t.id },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            RadioButton(selected = t.id == selected, onClick = null, modifier = Modifier.padding(end = 12.dp))
            Column {
                Text(t.label)
                t.detail?.let {
                    Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
    }
    Row(verticalAlignment = Alignment.CenterVertically) {
        OutlinedButton(enabled = bluetoothOn && !searching, onClick = { find() }) {
            Text(if (searching) "Searching…" else "Find the dongle")
        }
        if (searching) CircularProgressIndicator(Modifier.height(20.dp).padding(start = 12.dp), strokeWidth = 2.dp)
    }
    Text(
        "Switch the ignition on first. A dongle takes a new phone within 2 minutes of that (it then blinks green and blue). " +
            "Don't pair it in the phone's Bluetooth settings: OpenPlug pairs it when you connect.",
        Modifier.padding(top = 4.dp),
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    Spacer(Modifier.height(12.dp))
    OutlinedTextField(
        value = pin,
        onValueChange = { pin = it.trim() },
        label = { Text("PIN from the dongle label") },
        singleLine = true,
        visualTransformation = PasswordVisualTransformation(),
        // label PINs are alphanumeric (ours is 8 capital letters)
        keyboardOptions = KeyboardOptions(
            capitalization = KeyboardCapitalization.Characters,
            keyboardType = KeyboardType.Password,
        ),
        modifier = Modifier.fillMaxWidth(),
    )
    Spacer(Modifier.height(12.dp))
    val target = all.firstOrNull { it.id == selected }
    Button(
        enabled = target != null && pin.isNotEmpty() && !pairing,
        onClick = {
            val t = target!!
            stopSearch?.invoke()
            askPermissions.launch(wantedPermissions()) // notifications, if not asked yet
            Settings.setTargetName(context, t.label)
            val start = { Session.start(context, SessionSettings(t.id, pin, Settings.record(context))) }
            if (t.id == Settings.FAKE_ID) {
                start()
            } else {
                // first contact: pair, as the original app did (the connection is tried either way)
                pairing = true
                DongleFinder.pair(context, t.id) {
                    pairing = false
                    start()
                }
            }
        },
    ) { Text(if (pairing) "Pairing…" else "Connect") }
    if (pairing) {
        Text(
            "If the phone asks to pair with the dongle, confirm it.",
            Modifier.padding(top = 8.dp),
            style = MaterialTheme.typography.bodyMedium,
        )
    }
    (state as? LinkState.Failed)?.let {
        Spacer(Modifier.height(8.dp))
        Text(it.message, color = MaterialTheme.colorScheme.error)
    }
}

// ---- car: tank size, service interval ----

@Composable
private fun CarSection() {
    val journal by Session.journal.collectAsStateWithLifecycle()
    val car = journal.car
    fun plain(v: Double?) = v?.let { if (it % 1.0 == 0.0) it.toLong().toString() else it.toString() } ?: ""
    var tank by rememberSaveable { mutableStateOf(plain(car.tankL)) }
    var everyKm by rememberSaveable { mutableStateOf(plain(car.serviceEveryKm)) }
    val months = car.serviceEveryDays?.let { Math.round(it.toDouble() / 30.4375).toInt() } ?: 0

    fun parse(text: String): Double? = text.filterNot { it.isWhitespace() }.replace(',', '.').toDoubleOrNull()
    fun save(tankText: String = tank, kmText: String = everyKm, everyMonths: Int = months) {
        val days = if (everyMonths > 0) Math.round(everyMonths * 30.4375).toUInt() else null
        val settings = CarSettings(parse(tankText), parse(kmText), days)
        if (settings != journal.car) Session.setCar(settings)
    }

    OutlinedTextField(
        value = tank,
        onValueChange = { tank = it; save(tankText = it) },
        label = { Text("Tank size") },
        suffix = { Text("l") },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
        modifier = Modifier.fillMaxWidth(),
    )
    Spacer(Modifier.height(8.dp))
    OutlinedTextField(
        value = everyKm,
        onValueChange = { everyKm = it; save(kmText = it) },
        label = { Text("Service every") },
        suffix = { Text("km") },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
        modifier = Modifier.fillMaxWidth(),
    )
    Text("or every", Modifier.padding(top = 12.dp, bottom = 4.dp))
    val choices = listOf(0 to "–", 6 to "6 mo", 12 to "1 yr", 24 to "2 yr", 36 to "3 yr")
    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
        choices.forEachIndexed { i, (m, label) ->
            SegmentedButton(
                selected = months == m,
                onClick = { save(everyMonths = m) },
                shape = SegmentedButtonDefaults.itemShape(i, choices.size),
                icon = {},
            ) { Text(label) }
        }
    }
}

// ---- recording: background, value recordings ----

@Composable
private fun RecordingSection() {
    val context = LocalContext.current
    val state by Session.state.collectAsStateWithLifecycle()
    var unrestricted by remember { mutableStateOf(Settings.isBatteryUnrestricted(context)) }
    LifecycleResumeEffect(Unit) {
        unrestricted = Settings.isBatteryUnrestricted(context)
        onPauseOrDispose {}
    }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text("Runs in the background")
            Text(
                if (unrestricted) {
                    "Every drive is recorded without opening the app."
                } else {
                    "Allow OpenPlug to run without battery restrictions, so it can wake up for the car."
                },
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        if (unrestricted) {
            Text("Allowed", Modifier.padding(start = 12.dp), fontWeight = FontWeight.SemiBold, color = MaterialTheme.colorScheme.primary)
        }
    }
    if (!unrestricted) {
        Spacer(Modifier.height(8.dp))
        OutlinedButton(onClick = {
            @SuppressLint("BatteryLife") // a trip logger has to wake for the car; the user decides
            val intent = Intent(
                android.provider.Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS,
                Uri.parse("package:${context.packageName}"),
            )
            context.startActivity(intent)
        }) { Text("Allow") }
    }
    Divider()
    RoutesRow()
    Divider()
    var addresses by remember { mutableStateOf(Settings.lookUpAddresses(context)) }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text("Addresses for trips")
            Text(
                "Looks up the street address of each trip's start and end, and of the parking place. Sends those positions to the phone's address service, or to OpenStreetMap (Nominatim) if the phone has none.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Switch(checked = addresses, modifier = Modifier.padding(start = 12.dp), onCheckedChange = {
            addresses = it
            Settings.setLookUpAddresses(context, it)
            if (it) Session.lookUpAddresses()
        })
    }
    Divider()
    var record by remember { mutableStateOf(Settings.record(context)) }
    val idle = state is LinkState.Idle || state is LinkState.Failed
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text("Record values for replay")
            Text(
                "For bug reports and testing. No position, no VIN." + if (idle) "" else " Applies from the next connect.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Switch(checked = record, modifier = Modifier.padding(start = 12.dp), onCheckedChange = {
            record = it
            Settings.setRecord(context, it)
        })
    }
}

/** "Record routes": the switch, and the location permissions it needs. */
@Composable
private fun RoutesRow() {
    val context = LocalContext.current
    var on by remember { mutableStateOf(Settings.recordRoutes(context)) }
    var granted by remember { mutableStateOf(PositionFeed.hasPermission(context)) }
    var background by remember { mutableStateOf(PositionFeed.worksInBackground(context)) }
    LifecycleResumeEffect(Unit) {
        granted = PositionFeed.hasPermission(context)
        background = PositionFeed.worksInBackground(context)
        onPauseOrDispose {}
    }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {
        granted = PositionFeed.hasPermission(context)
        background = PositionFeed.worksInBackground(context)
    }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text("Record routes")
            Text(
                when {
                    !on -> "Uses GPS while the car is connected: trips on a map and where the car is parked. Stays on this phone."
                    !granted -> "Location is not allowed for OpenPlug yet."
                    !background -> "Allowed only while the app is open. For drives recorded without opening the app, allow location all the time."
                    else -> "Recorded while the car is connected. Applies from the next connect."
                },
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Switch(checked = on, modifier = Modifier.padding(start = 12.dp), onCheckedChange = {
            on = it
            Settings.setRecordRoutes(context, it)
            if (it && !granted) {
                ask.launch(arrayOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION))
            }
        })
    }
    if (on && !granted) {
        OutlinedButton(onClick = {
            ask.launch(arrayOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION))
        }) { Text("Allow location") }
    } else if (on && !background && Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
        // has to be asked for on its own, after the foreground permission
        OutlinedButton(onClick = { ask.launch(arrayOf(Manifest.permission.ACCESS_BACKGROUND_LOCATION)) }) {
            Text("Allow all the time")
        }
    }
}

// ---- data: live signals, export, recordings ----

@Composable
private fun DataSection(onSignals: () -> Unit) {
    val context = LocalContext.current
    val readings by Session.readings.collectAsStateWithLifecycle()
    val journal by Session.journal.collectAsStateWithLifecycle()
    LinkRow("All live signals", if (readings.isEmpty()) "" else readings.size.toString(), onClick = onSignals)
    Divider()
    LinkRow("Export trips", if (journal.trips.isEmpty()) "none yet" else "CSV", enabled = journal.trips.isNotEmpty()) {
        val csv = Session.exportCsv() ?: return@LinkRow
        val send = Intent(Intent.ACTION_SEND)
            .setType("text/plain")
            .putExtra(Intent.EXTRA_SUBJECT, "OpenPlug trips")
            .putExtra(Intent.EXTRA_TEXT, csv)
        context.startActivity(Intent.createChooser(send, "Export trips"))
    }
    Divider()
    val files = remember { Session.recordingsDir(context).listFiles()?.size ?: 0 }
    Row {
        Column(Modifier.weight(1f)) {
            Text("Recordings")
            Text(
                "Android/data/${context.packageName}/files/recordings",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Text(if (files == 1) "1 file" else "$files files", color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
    Divider()
    EraseRow()
}

/** "Erase all data", behind a confirmation. */
@Composable
private fun EraseRow() {
    val context = LocalContext.current
    val erasing by Session.erasing.collectAsStateWithLifecycle()
    val state by Session.state.collectAsStateWithLifecycle()
    var asking by remember { mutableStateOf(false) }
    Row(
        Modifier.fillMaxWidth().heightIn(min = 44.dp).clickable(enabled = !erasing) { asking = true },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(if (erasing) "Erasing…" else "Erase all data", Modifier.weight(1f), color = MaterialTheme.colorScheme.error)
        if (erasing) CircularProgressIndicator(Modifier.height(20.dp), strokeWidth = 2.dp)
    }
    if (asking) {
        val connected = state !is LinkState.Idle && state !is LinkState.Failed
        AlertDialog(
            onDismissRequest = { asking = false },
            title = { Text("Erase all data?") },
            text = {
                Text(
                    (if (connected) "Disconnects from the dongle, then deletes" else "Deletes") +
                        " all trips with their routes and addresses, refuels, the car's last values and settings," +
                        " value recordings, the stored PIN and every setting from this phone. This cannot be undone" +
                        " — export the trips first if you want to keep them.\n\n" +
                        "Permissions and the Bluetooth pairing stay; change those in the system settings.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    asking = false
                    Session.eraseAll(context)
                }) { Text("Erase", color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { asking = false }) { Text("Cancel") } },
        )
    }
}

@Composable
private fun LinkRow(label: String, detail: String, enabled: Boolean = true, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 44.dp).clickable(enabled = enabled, onClick = onClick),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, Modifier.weight(1f))
        Text(detail, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** Every data object the dongle streams, labelled or not: the diagnostic view. */
@Composable
private fun Signals(onBack: () -> Unit) {
    val readings by Session.readings.collectAsStateWithLifecycle()
    Column(Modifier.fillMaxSize().padding(horizontal = 20.dp, vertical = 8.dp)) {
        TextButton(onClick = onBack, contentPadding = PaddingValues(0.dp)) { Text("‹ Settings") }
        ScreenTitle("Live signals")
        Spacer(Modifier.height(12.dp))
        if (readings.isEmpty()) {
            Text("Nothing yet — the values appear while the car is connected.", color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        LazyColumn {
            items(readings) { r -> // no keys: a keyed list would scroll away from rows inserted above
                Row(Modifier.fillMaxWidth().padding(vertical = 12.dp)) {
                    Text(r.name ?: String.format(Locale.ROOT, "0x%06x", r.url.toInt()), Modifier.weight(1f))
                    Text(formatValue(r), fontWeight = FontWeight.SemiBold)
                }
                HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
            }
        }
    }
}

private fun formatValue(r: Reading): String {
    if (!r.available) return "—"
    val number = String.format(Locale.getDefault(), "%.${r.precision.toInt()}f", r.value)
    return if (r.unit != null) "$number ${r.unit}" else number
}
