package de.kobin.sourcerenderer

import androidx.appcompat.app.AppCompatActivity
import android.os.Bundle
import android.os.PowerManager

class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.setSustainedPerformanceMode(true)

        setContentView(R.layout.activity_main)

        System.loadLibrary("sourcerenderer");
        onCreateNative()
    }

    private external fun onCreateNative()
}
