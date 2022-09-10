package de.kobin.sourcerenderer

import android.app.ActivityManager
import android.app.ApplicationExitInfo
import android.app.GameManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.view.*
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.documentfile.provider.DocumentFile

class MainActivity : AppCompatActivity() {
    private var enginePtr: Long = 0

    companion object {
        private const val TAG = "SourceRenderer"
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.setSustainedPerformanceMode(true)

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val am =
                applicationContext.getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager
            val exitList = am.getHistoricalProcessExitReasons(applicationContext.packageName, 0, 1)
            if (exitList.isNotEmpty()) {
                val lastExitInformation: ApplicationExitInfo = exitList.first()
                Log.e(TAG, "Crashed: $lastExitInformation")
            }
        }


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

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            val gameManager: GameManager? = getSystemService(Context.GAME_SERVICE) as GameManager?
            // Returns the selected GameMode
            val gameMode = gameManager?.gameMode
            Log.d("SourceRenderer", "Game mode: $gameMode")
        }


        //askForCsgoDirectory()

        val holder = view.holder
        holder.addCallback(object: SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                    holder.surface.setFrameRate(0f, Surface.FRAME_RATE_COMPATIBILITY_DEFAULT)
                }
                if (this@MainActivity.enginePtr != 0L) {
                    return
                }
                this@MainActivity.enginePtr = startEngineNative(holder.surface!!)
            }

            override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
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
        if (this.enginePtr != 0L) {
            onDestroyNative(this.enginePtr)
        }
    }

    override fun onLowMemory() {
        super.onLowMemory()
        Log.w(TAG, "Warning: Low memory!")
        // TODO: Implement
    }

    override fun onTouchEvent(event: MotionEvent?): Boolean {
        if (super.onTouchEvent(event)) {
            return true
        }
        event ?: return false
        if (this.enginePtr == 0L) {
            return false
        }
        this.onTouchInputNative(this.enginePtr, event.x, event.y, event.actionIndex, event.actionMasked)
        return true
    }

    private fun pickDirectory(callback: (Uri) -> Void) {
        registerForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
            uri ?: return@registerForActivityResult
            contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
            val tree = DocumentFile.fromTreeUri(applicationContext, uri)!!
            val file = tree.listFiles().firstOrNull { it != null } ?: return@registerForActivityResult
            val filePath = file.uri.toString()
            var dir = if (file.name != null) {
                filePath.substring(0, filePath.lastIndexOf(file.name!!))
            } else {
                filePath
            }
            if (dir.endsWith("%2F")) {
                dir = dir.substring(0, dir.length - "%2F".length)
            }
            callback(dir)
        }.launch(null)
    }

    private external fun startEngineNative(surface: Surface): Long
    private external fun onSurfaceChangedNative(enginePtr: Long, surface: Surface?): Long
    private external fun onTouchInputNative(enginePtr: Long, x: Float, y: Float, fingerIndex: Int, eventType: Int)
    private external fun onDestroyNative(enginePtr: Long)
}
