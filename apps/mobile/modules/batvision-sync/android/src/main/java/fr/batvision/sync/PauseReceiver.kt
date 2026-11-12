package fr.batvision.sync

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/** The notification's Pause button. */
class PauseReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    Engine.get(context).setPaused(true)
  }
}
