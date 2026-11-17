plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "dev.rustbrowser"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.rustbrowser"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
    }

    signingConfigs {
        // Свой ключ для домашних сборок: лежит рядом с проектом, в git не попадает.
        create("home") {
            storeFile = file("../rustbrowser.keystore")
            storePassword = "rustbrowser"
            keyAlias = "rustbrowser"
            keyPassword = "rustbrowser"
        }
    }

    buildTypes {
        release {
            // Скорость: R8 выкидывает лишнее и сжимает код, ресурсы — тоже.
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.getByName("home")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    buildFeatures { compose = true }

    // Список рекламных доменов — тот же файл, что у настольной версии.
    sourceSets["main"].assets.srcDir(layout.buildDirectory.dir("generated/shared-assets"))
}

val copySharedAssets by tasks.registering(Copy::class) {
    from("../../src/blocklist.txt")
    into(layout.buildDirectory.dir("generated/shared-assets"))
}
tasks.named("preBuild") { dependsOn(copySharedAssets) }

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2024.12.01")
    implementation(composeBom)
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.animation:animation")
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.webkit:webkit:1.12.1")
    // готовые профили запуска Compose ставятся при установке — первый запуск быстрее
    implementation("androidx.profileinstaller:profileinstaller:1.4.1")
}
