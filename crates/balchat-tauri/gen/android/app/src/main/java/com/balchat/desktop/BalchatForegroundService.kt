package com.balchat.desktop

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Intent
import android.os.Build
import android.os.IBinder

/**
 * Foreground service que muestra una notification persistent. Su único trabajo es
 * mantener al proceso de balchat por encima de la "cached" priority de Android para
 * que NO mate al proceso cuando la app pasa a background — esto preserva el cliente
 * Tor (Arti) corriendo dentro de tokio en el código Rust.
 *
 * El service NO arranca Tor por sí mismo. Arti vive en el código Rust del binario,
 * en `lib.rs::run_daemon` cuando el frontend invoca el comando `start_daemon`. El
 * service simplemente "atra" el proceso vivo.
 */
class BalchatForegroundService : Service() {
  companion object {
    const val CHANNEL_ID = "balchat_persistent"
    const val NOTIFICATION_ID = 0xBA1C
  }

  override fun onCreate() {
    super.onCreate()
    createChannelIfNeeded()
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    val openAppIntent = Intent(this, MainActivity::class.java)
    openAppIntent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP)
    val pending = PendingIntent.getActivity(
      this,
      0,
      openAppIntent,
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
    )

    val notification: Notification = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      Notification.Builder(this, CHANNEL_ID)
        .setContentTitle("balchat activo")
        .setContentText("Manteniendo Tor y daemon vivos en background")
        .setSmallIcon(android.R.drawable.ic_dialog_info)
        .setContentIntent(pending)
        .setOngoing(true)
        .setCategory(Notification.CATEGORY_SERVICE)
        .build()
    } else {
      @Suppress("DEPRECATION")
      Notification.Builder(this)
        .setContentTitle("balchat activo")
        .setContentText("Manteniendo Tor y daemon vivos en background")
        .setSmallIcon(android.R.drawable.ic_dialog_info)
        .setContentIntent(pending)
        .setOngoing(true)
        .build()
    }

    startForeground(NOTIFICATION_ID, notification)
    // START_STICKY: si el OS mata el service, lo recrea automáticamente.
    return START_STICKY
  }

  override fun onBind(intent: Intent?): IBinder? = null

  private fun createChannelIfNeeded() {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      val ch = NotificationChannel(
        CHANNEL_ID,
        "balchat persistente",
        NotificationManager.IMPORTANCE_LOW
      ).apply {
        description = "Mantiene el proceso vivo cuando la app está en background"
        setShowBadge(false)
      }
      getSystemService(NotificationManager::class.java).createNotificationChannel(ch)
    }
  }
}
