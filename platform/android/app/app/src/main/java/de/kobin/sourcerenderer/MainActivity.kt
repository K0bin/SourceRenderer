package de.kobin.sourcerenderer

import android.os.Build
import androidx.appcompat.app.AppCompatActivity
import android.os.Bundle
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView

class MainActivity : AppCompatActivity() {
    private var nativeBridgePtr: Long = 0

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.setSustainedPerformanceMode(true)

        setContentView(R.layout.activity_main)

        val view = findViewById<SurfaceView>(R.id.surface);
        val holder = view.holder
        holder.addCallback(object: SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {
                // handled by surfaceChanged
            }

            override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                    holder.surface.setFrameRate(0f, Surface.FRAME_RATE_COMPATIBILITY_DEFAULT)
                }
                onSurfaceChangedNative(nativeBridgePtr, holder.surface)
            }

            override fun surfaceDestroyed(holder: SurfaceHolder) {
                // TODO
            }
        })

        System.loadLibrary("sourcerenderer");
        this.nativeBridgePtr = onCreateNative()
    }

    override fun onDestroy() {
        super.onDestroy()
        onDestroyNative(this.nativeBridgePtr)
    }

    private external fun onCreateNative(): Long
    private external fun onSurfaceChangedNative(nativePtr: Long, surface: Surface): Long
    private external fun onDestroyNative(nativePtr: Long)
}
