package de.kobin.sourcerenderer

import android.content.res.AssetManager
import android.os.Build
import androidx.appcompat.app.AppCompatActivity
import android.os.Bundle
import android.util.Log
import android.view.*

class MainActivity : AppCompatActivity() {
    private var enginePtr: Long = 0

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.setSustainedPerformanceMode(true)

        setContentView(R.layout.activity_main)

        val view = findViewById<SurfaceView>(R.id.surface)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            window.setDecorFitsSystemWindows(false)
            view.windowInsetsController?.apply {
                this.systemBarsBehavior = WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
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
                if (this@MainActivity.enginePtr != 0L) {
                    Log.e("MainActivity", "Engine initialized when surfaceCreated was called.")
                    return
                }
                this@MainActivity.enginePtr = startEngineNative(holder.surface)
            }

            override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                    holder.surface.setFrameRate(0f, Surface.FRAME_RATE_COMPATIBILITY_DEFAULT)
                }
                if (this@MainActivity.enginePtr == 0L) {
                    return
                }
                onSurfaceChangedNative(this@MainActivity.enginePtr, holder.surface)
            }

            override fun surfaceDestroyed(holder: SurfaceHolder) {
                if (this@MainActivity.enginePtr == 0L) {
                    return
                }
                onSurfaceChangedNative(this@MainActivity.enginePtr, null)
            }
        })
    }

    override fun onDestroy() {
        super.onDestroy()
        onDestroyNative(this.enginePtr)
    }

    override fun onTouchEvent(event: MotionEvent?): Boolean {
        if (super.onTouchEvent(event)) {
            return true
        }
        event ?: return false
        this.onTouchInputNative(event.x, event.y, event.actionIndex, event.actionMasked)
        return true
    }

    private external fun startEngineNative(surface: Surface): Long
    private external fun onSurfaceChangedNative(enginePtr: Long, surface: Surface?): Long
    private external fun onTouchInputNative(x: Float, y: Float, fingerIndex: Int, eventType: Int)
    private external fun onDestroyNative(enginePtr: Long)
}
