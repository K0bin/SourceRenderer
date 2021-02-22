@file:Suppress("unused") // used in native code

package de.kobin.sourcerenderer

import android.content.Context
import android.net.Uri
import android.util.Log
import androidx.annotation.Keep
import java.io.FileNotFoundException

@Keep
object IO {
    @JvmStatic
    var applicationContext: Context? = null

    @Keep
    @JvmStatic
    fun openFile(path: String): Int {
        applicationContext ?: return 0
        return try {
            val parcel = applicationContext!!.contentResolver.openFileDescriptor(Uri.parse(path), "r")
            parcel?.detachFd() ?: 0
        } catch (e: FileNotFoundException) {
            Log.d("SourceRenderer", "External asset file not found $e");
            -1
        } catch (e: SecurityException) {
            Log.d("SourceRenderer", "External asset security exception $e");
            -2
        }
    }
}
