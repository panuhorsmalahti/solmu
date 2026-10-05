plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "com.solmu.android"
    compileSdk = 36

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        applicationId = "com.solmu.android"
        minSdk = 26
        targetSdk = 35
        versionCode = providers.gradleProperty("solmuVersionCode").orElse("1").get().toInt()
        versionName = providers.gradleProperty("solmuVersionName").orElse("0.1.0").get()
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    signingConfigs {
        create("release") {
            val keystoreFile = providers.environmentVariable("SOLMU_ANDROID_KEYSTORE_FILE")
            val keyAlias = providers.environmentVariable("SOLMU_ANDROID_KEY_ALIAS")
            val keyPassword = providers.environmentVariable("SOLMU_ANDROID_KEY_PASSWORD")
            val storePassword = providers.environmentVariable("SOLMU_ANDROID_STORE_PASSWORD")
            if (keystoreFile.isPresent && keyAlias.isPresent && keyPassword.isPresent && storePassword.isPresent) {
                storeFile = file(keystoreFile.get())
                this.keyAlias = keyAlias.get()
                this.keyPassword = keyPassword.get()
                this.storePassword = storePassword.get()
            }
        }
    }

    buildTypes {
        release {
            signingConfig = signingConfigs.getByName("release")
        }
    }

    buildFeatures {
        compose = true
    }
}

android.sourceSets.getByName("androidTest").java.srcDir("../../../e2e/android")

dependencies {
    implementation(platform("androidx.compose:compose-bom:2025.10.01"))
    implementation("androidx.activity:activity-compose:1.11.0")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.9.4")
    implementation("com.squareup.okhttp3:okhttp:4.12.0")

    debugImplementation("androidx.compose.ui:ui-tooling")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
    androidTestImplementation(platform("androidx.compose:compose-bom:2025.10.01"))
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
    androidTestImplementation("androidx.test:runner:1.7.0")
    androidTestImplementation("androidx.test:core-ktx:1.7.0")
    androidTestImplementation("com.squareup.okhttp3:mockwebserver:4.12.0")
}
