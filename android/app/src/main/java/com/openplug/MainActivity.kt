package com.openplug

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import java.util.Locale

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        Session.openJournal(applicationContext)
        Session.resume(applicationContext) // a wanted session that is not running (process was killed)
        setContent {
            MaterialTheme(if (isSystemInDarkTheme()) DarkColors else LightColors) {
                OpenPlugScreen()
            }
        }
    }
}

// One calm look: a quiet ground, white panels, one petrol-blue accent, amber for
// refuels and reminders.
private val LightColors = lightColorScheme(
    primary = Color(0xFF0B6E99),
    onPrimary = Color.White,
    primaryContainer = Color(0xFFE1EFF5),
    onPrimaryContainer = Color(0xFF08506F),
    secondaryContainer = Color(0xFFE1EFF5),
    onSecondaryContainer = Color(0xFF08506F),
    tertiary = Color(0xFF8A4A07),
    tertiaryContainer = Color(0xFFFBEBD6),
    onTertiaryContainer = Color(0xFF8A4A07),
    background = Color(0xFFF4F5F2),
    onBackground = Color(0xFF15181C),
    surface = Color(0xFFF4F5F2),
    onSurface = Color(0xFF15181C),
    onSurfaceVariant = Color(0xFF4D5550),
    surfaceContainerLowest = Color.White,
    surfaceContainer = Color.White,
    surfaceContainerHighest = Color(0xFFE8EBE6),
    outlineVariant = Color(0xFFE2E5E0),
    inverseSurface = Color(0xFF15181C),
    inverseOnSurface = Color(0xFFF4F5F2),
    error = Color(0xFFB42318),
)

private val DarkColors = darkColorScheme(
    primary = Color(0xFF7CC7E8),
    onPrimary = Color(0xFF00354B),
    primaryContainer = Color(0xFF0D3A4D),
    onPrimaryContainer = Color(0xFFCDEBF7),
    secondaryContainer = Color(0xFF0D3A4D),
    onSecondaryContainer = Color(0xFFCDEBF7),
    tertiary = Color(0xFFF0B46A),
    tertiaryContainer = Color(0xFF4A2C06),
    onTertiaryContainer = Color(0xFFF7D3A4),
    background = Color(0xFF101315),
    surface = Color(0xFF101315),
    surfaceContainerLowest = Color(0xFF1A1E21),
    surfaceContainer = Color(0xFF1A1E21),
    surfaceContainerHighest = Color(0xFF2A2F33),
    outlineVariant = Color(0xFF2C3136),
    // the "dark card" of the light theme stays a raised dark card here
    inverseSurface = Color(0xFF23282C),
    inverseOnSurface = Color(0xFFF4F5F2),
)

@Composable
fun OpenPlugScreen() {
    var tab by rememberSaveable { mutableIntStateOf(0) }
    Scaffold(
        bottomBar = {
            NavigationBar(containerColor = MaterialTheme.colorScheme.surfaceContainerLowest) {
                listOf("Car" to R.drawable.ic_car, "Trips" to R.drawable.ic_trips, "Settings" to R.drawable.ic_settings)
                    .forEachIndexed { i, (label, icon) ->
                        NavigationBarItem(
                            selected = tab == i,
                            onClick = { tab = i },
                            icon = { Icon(painterResource(icon), contentDescription = null) },
                            label = { Text(label) },
                        )
                    }
            }
        },
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding)) {
            when (tab) {
                0 -> CarTab(onSetUp = { tab = 2 })
                1 -> TripsTab()
                else -> SettingsTab()
            }
        }
    }
}

// ---- pieces shared by the tabs ----

@Composable
fun ScreenTitle(text: String, modifier: Modifier = Modifier) {
    Text(text, modifier, style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
}

/** A white, bordered, rounded panel. */
@Composable
fun Panel(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(18.dp),
        color = MaterialTheme.colorScheme.surfaceContainerLowest,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
    ) { Column(Modifier.padding(16.dp), content = content) }
}

/** A small caption above a panel or beside a value. */
@Composable
fun Caption(text: String, modifier: Modifier = Modifier) {
    Text(
        text,
        modifier,
        style = MaterialTheme.typography.labelLarge,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

fun number(value: Double, decimals: Int): String = String.format(Locale.getDefault(), "%,.${decimals}f", value)

fun km(v: Double): String = number(v, if (v < 100) 1 else 0) + " km"

fun litres(v: Double): String = number(v, 1) + " l"

fun duration(seconds: ULong): String {
    val m = (seconds.toLong() + 30) / 60
    return if (m < 60) "$m min" else "${m / 60} h ${m % 60} min"
}
