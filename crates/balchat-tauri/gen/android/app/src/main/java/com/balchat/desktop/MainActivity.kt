package com.balchat.desktop

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import androidx.activity.enableEdgeToEdge
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat

class MainActivity : TauriActivity() {
  companion object {
    private const val REQ_POST_NOTIFICATIONS = 0x10A
  }

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // Diferimos 2s para no bloquear el primer frame (Tauri carga su WebView en
    // main thread y eso de por sí ya consume varios segundos).
    Handler(Looper.getMainLooper()).postDelayed({
      maybeRequestPostNotifications()
      startBalchatForegroundService()
    }, 2000)
  }

  /**
   * Android 13+ (Tiramisu, API 33) exige `POST_NOTIFICATIONS` como runtime
   * permission. Sin esto, la notification persistente del foreground service
   * no se muestra (aunque el service sí queda corriendo en background).
   */
  private fun maybeRequestPostNotifications() {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
      val granted = ContextCompat.checkSelfPermission(
        this, Manifest.permission.POST_NOTIFICATIONS
      ) == PackageManager.PERMISSION_GRANTED
      if (!granted) {
        ActivityCompat.requestPermissions(
          this,
          arrayOf(Manifest.permission.POST_NOTIFICATIONS),
          REQ_POST_NOTIFICATIONS
        )
      }
    }
  }

  private fun startBalchatForegroundService() {
    val serviceIntent = Intent(this, BalchatForegroundService::class.java)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      startForegroundService(serviceIntent)
    } else {
      startService(serviceIntent)
    }
  }
}
