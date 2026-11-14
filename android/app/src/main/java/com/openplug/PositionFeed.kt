package com.openplug

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.location.LocationRequest
import android.os.Build
import android.os.Looper
import android.util.Log

/**
 * The phone's position while the car is connected, for the route of a trip and the
 * place the car was left. Uses the platform location service (no Google services).
 * Runs only between [start] and [stop], and only if the user switched on "Record
 * routes" and granted the permission; nothing leaves the device.
 */
object PositionFeed {
    private val listener = LocationListener { fix: Location ->
        if (!fix.hasAccuracy() || fix.accuracy <= 50f) Session.onPosition(fix.latitude, fix.longitude)
    }

    fun hasPermission(context: Context): Boolean =
        context.checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED

    /** Whether fixes also arrive when the session was resumed in the background. */
    fun worksInBackground(context: Context): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.Q ||
            context.checkSelfPermission(Manifest.permission.ACCESS_BACKGROUND_LOCATION) ==
            PackageManager.PERMISSION_GRANTED

    fun start(context: Context) {
        if (!Settings.recordRoutes(context) || !hasPermission(context)) return
        val manager = context.getSystemService(LocationManager::class.java) ?: return
        val provider = when {
            Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && manager.hasProvider(LocationManager.FUSED_PROVIDER) ->
                LocationManager.FUSED_PROVIDER
            else -> LocationManager.GPS_PROVIDER
        }
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                val request = LocationRequest.Builder(2_000L)
                    .setQuality(LocationRequest.QUALITY_HIGH_ACCURACY) // a route needs GPS, not cell towers
                    .setMinUpdateDistanceMeters(10f)
                    .build()
                manager.requestLocationUpdates(provider, request, context.mainExecutor, listener)
            } else {
                manager.requestLocationUpdates(provider, 2_000L, 10f, listener, Looper.getMainLooper())
            }
        } catch (e: RuntimeException) { // permission withdrawn, or no such provider
            Log.w("OpenPlug", "no position updates: $e")
        }
    }

    fun stop(context: Context) {
        context.getSystemService(LocationManager::class.java)?.removeUpdates(listener)
    }
}
