package com.openplug

import android.content.Context
import android.location.Address
import android.location.Geocoder
import android.util.Log
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import java.util.Locale

/**
 * Street addresses for positions. Uses the phone's own geocoder when it has one
 * (most phones with Google services); otherwise OpenStreetMap's Nominatim, at most
 * one request a second as its usage policy asks. Used only when the user switches it
 * on: each lookup sends one position to that service. Blocking: call off the main thread.
 */
object Addresses {
    private const val TAG = "OpenPlug"
    private val cache = HashMap<String, String>()
    private var lastNominatimMs = 0L

    /**
     * The address of a place, or "" when there is none (open country); null when the
     * lookup failed (offline, refused) and is worth trying again later.
     */
    fun lookUp(context: Context, lat: Double, lon: Double): String? {
        val key = String.format(Locale.ROOT, "%.4f,%.4f", lat, lon) // ~10 m
        synchronized(cache) { cache[key] }?.let { return it }
        val found = if (Geocoder.isPresent()) platform(context, lat, lon) else nominatim(context, lat, lon)
        if (found != null) synchronized(cache) { cache[key] = found }
        return found
    }

    @Suppress("DEPRECATION") // the listener variant needs API 33; this runs on a worker thread
    private fun platform(context: Context, lat: Double, lon: Double): String? = try {
        Geocoder(context, Locale.getDefault()).getFromLocation(lat, lon, 1)?.firstOrNull()?.let(::format) ?: ""
    } catch (e: Exception) {
        Log.w(TAG, "geocoder: $e")
        null
    }

    private fun format(a: Address): String {
        val street = listOfNotNull(a.thoroughfare, a.subThoroughfare).joinToString(" ")
        val first = street.ifEmpty { a.featureName.orEmpty() }
        return listOf(first, a.locality ?: a.subAdminArea.orEmpty()).filter { it.isNotEmpty() }.distinct().joinToString(", ")
    }

    private fun nominatim(context: Context, lat: Double, lon: Double): String? {
        synchronized(this) {
            val wait = lastNominatimMs + 1100 - System.currentTimeMillis()
            if (wait > 0) Thread.sleep(wait)
            lastNominatimMs = System.currentTimeMillis()
        }
        val url = URL(
            String.format(
                Locale.ROOT,
                "https://nominatim.openstreetmap.org/reverse?format=jsonv2&zoom=18&lat=%.6f&lon=%.6f&accept-language=%s",
                lat, lon, URLEncoder.encode(Locale.getDefault().toLanguageTag(), "UTF-8"),
            ),
        )
        return try {
            val conn = url.openConnection() as HttpURLConnection
            conn.connectTimeout = 10_000
            conn.readTimeout = 10_000
            // the usage policy asks for an identifying user agent
            conn.setRequestProperty("User-Agent", "OpenPlug/${BuildConfig.VERSION_NAME} (${context.packageName})")
            try {
                if (conn.responseCode != 200) return null
                val json = JSONObject(conn.inputStream.bufferedReader().use { it.readText() })
                val a = json.optJSONObject("address") ?: return ""
                fun get(vararg keys: String) = keys.firstNotNullOfOrNull { k -> a.optString(k).takeIf { it.isNotEmpty() } }
                val street = listOfNotNull(get("road", "pedestrian", "footway", "path"), get("house_number")).joinToString(" ")
                val first = street.ifEmpty { json.optString("name") }
                listOfNotNull(first.takeIf { it.isNotEmpty() }, get("city", "town", "village", "municipality"))
                    .distinct().joinToString(", ")
            } finally {
                conn.disconnect()
            }
        } catch (e: Exception) {
            Log.w(TAG, "nominatim: $e")
            null
        }
    }
}
