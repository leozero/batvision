package fr.batvision.sync

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import org.json.JSONArray
import org.json.JSONObject

/** An offline progress record, as `POST /api/progress/batch` takes it. */
data class PendingProgress(val kind: String, val id: Long, val positionMs: Long, val played: Boolean, val at: Long)

/**
 * What the engine keeps between runs: the server and its device token (encrypted with
 * a key that never leaves the Android keystore), pause, offline progress and the
 * monthly mobile data count.
 */
class Store(context: Context) {
  private val prefs = context.getSharedPreferences("batvision.sync", Context.MODE_PRIVATE)

  var server: String?
    get() = prefs.getString("server", null)
    set(value) = prefs.edit().putString("server", value).apply()

  var token: String?
    get() = prefs.getString("token", null)?.let { runCatching { decrypt(it) }.getOrNull() }
    set(value) = prefs.edit().putString("token", value?.let(::encrypt)).apply()

  /** This phone's device id on the server (from the sign-in). */
  var deviceId: Long?
    get() = prefs.getLong("deviceId", -1).takeIf { it >= 0 }
    set(value) = prefs.edit().apply { if (value == null) remove("deviceId") else putLong("deviceId", value) }.apply()

  var paused: Boolean
    get() = prefs.getBoolean("paused", false)
    set(value) = prefs.edit().putBoolean("paused", value).apply()

  /** Hardware AV1 decoding, detected once. */
  var av1: Boolean
    get() = prefs.getBoolean("av1", false)
    set(value) = prefs.edit().putBoolean("av1", value).apply()

  /** The last plan's settings, so rules apply even before the next plan. */
  var settings: JSONObject
    get() = prefs.getString("settings", null)?.let { runCatching { JSONObject(it) }.getOrNull() } ?: JSONObject()
    set(value) = prefs.edit().putString("settings", value.toString()).apply()

  @Synchronized
  fun mobileBytes(month: String): Long = if (prefs.getString("mobileMonth", null) == month) prefs.getLong("mobileBytes", 0) else 0

  @Synchronized
  fun addMobileBytes(month: String, bytes: Long) {
    val total = mobileBytes(month) + bytes
    prefs.edit().putString("mobileMonth", month).putLong("mobileBytes", total).apply()
  }

  @Synchronized
  fun pendingProgress(): List<PendingProgress> {
    val array = JSONArray(prefs.getString("pendingProgress", "[]"))
    return (0 until array.length()).map { array.getJSONObject(it) }.map {
      PendingProgress(it.getString("kind"), it.getLong("id"), it.getLong("position_ms"), it.getBoolean("played"), it.getLong("at"))
    }
  }

  /** Keeps only the latest record per item. */
  @Synchronized
  fun addProgress(p: PendingProgress) {
    val list = pendingProgress().filterNot { it.kind == p.kind && it.id == p.id } + p
    savePending(list)
  }

  @Synchronized
  fun removeProgress(sent: List<PendingProgress>) {
    savePending(pendingProgress().filterNot { p -> sent.any { it == p } })
  }

  private fun savePending(list: List<PendingProgress>) {
    prefs.edit().putString("pendingProgress", JSONArray(list.map(::progressJson)).toString()).apply()
  }

  fun clearSession() {
    prefs.edit().remove("token").remove("deviceId").remove("settings").remove("pendingProgress").apply()
  }

  // ---- Token encryption ------------------------------------------------------------------

  private fun key(): SecretKey {
    val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
    (store.getEntry(KEY_ALIAS, null) as? KeyStore.SecretKeyEntry)?.let { return it.secretKey }
    val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
    generator.init(
      KeyGenParameterSpec.Builder(KEY_ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
        .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
        .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
        .build(),
    )
    return generator.generateKey()
  }

  private fun encrypt(plain: String): String {
    val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
    val data = cipher.iv + cipher.doFinal(plain.toByteArray())
    return Base64.encodeToString(data, Base64.NO_WRAP)
  }

  private fun decrypt(encoded: String): String {
    val data = Base64.decode(encoded, Base64.NO_WRAP)
    val cipher = Cipher.getInstance("AES/GCM/NoPadding")
    cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, data, 0, 12))
    return String(cipher.doFinal(data, 12, data.size - 12))
  }

  companion object {
    private const val KEY_ALIAS = "batvision.token"

    fun progressJson(p: PendingProgress): JSONObject = JSONObject()
      .put("kind", p.kind).put("id", p.id).put("position_ms", p.positionMs).put("played", p.played).put("at", p.at)
  }
}
