plugins {
    alias(libs.plugins.kotlinMultiplatform)
    alias(libs.plugins.composeMultiplatform)
    alias(libs.plugins.composeCompiler)
}

kotlin {
    jvm()

    sourceSets {
        jvmMain.dependencies {
            implementation(compose.runtime)
            implementation(compose.ui)
            implementation(compose.desktop.currentOs)
            api(project(":wrywebview"))
        }
    }
}
