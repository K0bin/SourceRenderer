package de.kobin.sourcerenderer

import android.app.Application
import android.content.res.AssetManager

class App : Application() {
    override fun onCreate() {
        super.onCreate()

        System.loadLibrary("sourcerenderer")
        initNative(this.assets)
    }

    private external fun initNative(assetManager: AssetManager)
}