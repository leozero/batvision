package fr.batvision.sync

import java.time.Instant
import java.time.ZoneId

/** What may download over mobile data (D68). */
enum class MobileData {
  OFF,
  LIGHT,
  ALL;

  companion object {
    fun parse(value: String?) = when (value) {
      "light" -> LIGHT
      "all" -> ALL
      else -> OFF
    }
  }
}

/** The device's download rules, from the server's settings. */
data class Rules(val mobileData: MobileData, val capBytes: Long?, val chargingOnly: Boolean)

/** The connection and power right now. */
data class Conditions(val connected: Boolean, val metered: Boolean, val charging: Boolean)

/** Why a download can't start now. `null` = it can. */
enum class Wait {
  OFFLINE,
  WIFI,
  CHARGING,
  MOBILE_CAP,
}

object NetworkRules {
  /** Lighter profiles may use mobile data in `LIGHT` mode; `max` waits for Wi-Fi. */
  private val LIGHT_PROFILES = setOf("mobile", "saver")

  fun check(profile: String, rules: Rules, now: Conditions, mobileBytesThisMonth: Long): Wait? {
    if (!now.connected) return Wait.OFFLINE
    if (rules.chargingOnly && !now.charging) return Wait.CHARGING
    if (!now.metered) return null
    return when {
      rules.mobileData == MobileData.OFF -> Wait.WIFI
      rules.mobileData == MobileData.LIGHT && profile !in LIGHT_PROFILES -> Wait.WIFI
      rules.capBytes != null && mobileBytesThisMonth >= rules.capBytes -> Wait.MOBILE_CAP
      else -> null
    }
  }

  /** The calendar month mobile data is counted in, e.g. `2026-11`. */
  fun month(epochMillis: Long, zone: ZoneId = ZoneId.systemDefault()): String {
    val date = Instant.ofEpochMilli(epochMillis).atZone(zone)
    return "%04d-%02d".format(date.year, date.monthValue)
  }
}
