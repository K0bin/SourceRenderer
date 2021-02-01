package de.kobin.sourcerenderer

import android.content.res.AssetManager
import android.os.Build
import androidx.appcompat.app.AppCompatActivity
import android.os.Bundle
import android.view.*

class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.setSustainedPerformanceMode(true)

        setContentView(R.layout.activity_main)

        val view = findViewById<SurfaceView>(R.id.surface)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            window.setDecorFitsSystemWindows(false)
            view.windowInsetsController?.apply {
                this.systemBarsBehavior = WindowInsetsController.BEHAVIOR_SHOW_BARS_BY_SWIPE
                this.hide(WindowInsets.Type.statusBars()
                        or WindowInsets.Type.navigationBars()
                        or WindowInsets.Type.systemGestures()
                        or WindowInsets.Type.displayCutout())
            }
        } else {
            view.systemUiVisibility = (View.SYSTEM_UI_FLAG_FULLSCREEN
                or View.SYSTEM_UI_FLAG_IMMERSIVE
                or View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                or View.SYSTEM_UI_FLAG_HIDE_NAVIGATION)
        }

        val holder = view.holder
        holder.addCallback(object: SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {
                // handled by surfaceChanged
            }

            override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                    holder.surface.setFrameRate(0f, Surface.FRAME_RATE_COMPATIBILITY_DEFAULT)
                }
                onSurfaceChangedNative(holder.surface)
            }

            override fun surfaceDestroyed(holder: SurfaceHolder) {
                // TODO
            }
        })

        System.loadLibrary("sourcerenderer");
        onCreateNative(this.assets)
    }

    override fun onDestroy() {
        super.onDestroy()
        onDestroyNative()
    }

    private external fun onCreateNative(assetManager: AssetManager)
    private external fun onSurfaceChangedNative(surface: Surface): Long
    private external fun onDestroyNative()
}
