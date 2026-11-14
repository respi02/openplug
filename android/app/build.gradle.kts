plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

import java.util.Properties

/**
 * Release signing, from a properties file that never enters the repository
 * (default `../captures/release-signing.properties`, or the path in
 * OPENPLUG_SIGNING): `storeFile`, `keyAlias`, `passwordFile`, the last two paths
 * relative to that file. Without it the release build is unsigned (as F-Droid
 * builds it, to sign with its own key).
 */
val signing: Properties? = (System.getenv("OPENPLUG_SIGNING")?.let(::file) ?: rootProject.file("../captures/release-signing.properties"))
    .takeIf { it.isFile }
    ?.let { f -> Properties().apply { f.inputStream().use(::load); setProperty("dir", f.parentFile.path) } }

android {
    namespace = "com.openplug"
    compileSdk = 37

    defaultConfig {
        applicationId = "com.openplug"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0-alpha.1"
        ndk { abiFilters += listOf("arm64-v8a", "armeabi-v7a") }
    }

    signingConfigs {
        if (signing != null) {
            create("release") {
                val dir = file(signing.getProperty("dir"))
                val password = dir.resolve(signing.getProperty("passwordFile")).readText().trim()
                storeFile = dir.resolve(signing.getProperty("storeFile"))
                storeType = "pkcs12"
                storePassword = password
                keyAlias = signing.getProperty("keyAlias")
                keyPassword = password
            }
        }
    }

    buildTypes {
        release {
            signingConfig = signingConfigs.findByName("release")
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2026.09.00"))
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.11.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.11.0")
    // OpenStreetMap view for trip routes: no API key, no Google services.
    implementation("org.osmdroid:osmdroid-android:6.1.20")
    // Runtime for the UniFFI-generated Kotlin bindings (see build-core.sh).
    implementation("net.java.dev.jna:jna:5.19.1@aar")
}
