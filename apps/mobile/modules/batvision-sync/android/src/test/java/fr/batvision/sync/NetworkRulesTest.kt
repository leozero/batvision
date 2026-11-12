package fr.batvision.sync

import java.time.ZoneId
import org.junit.Assert.assertEquals
import org.junit.Test

class NetworkRulesTest {
  private val wifi = Conditions(connected = true, metered = false, charging = false)
  private val mobile = Conditions(connected = true, metered = true, charging = false)
  private fun rules(mode: MobileData, cap: Long? = null, charging: Boolean = false) = Rules(mode, cap, charging)

  @Test
  fun `wifi downloads anything, offline nothing`() {
    assertEquals(null, NetworkRules.check("max", rules(MobileData.OFF), wifi, 0))
    assertEquals(Wait.OFFLINE, NetworkRules.check("saver", rules(MobileData.ALL), wifi.copy(connected = false), 0))
  }

  @Test
  fun `mobile data follows the device's mode`() {
    assertEquals(Wait.WIFI, NetworkRules.check("saver", rules(MobileData.OFF), mobile, 0))
    assertEquals(null, NetworkRules.check("mobile", rules(MobileData.LIGHT), mobile, 0))
    assertEquals(null, NetworkRules.check("saver", rules(MobileData.LIGHT), mobile, 0))
    assertEquals(Wait.WIFI, NetworkRules.check("max", rules(MobileData.LIGHT), mobile, 0))
    assertEquals(null, NetworkRules.check("max", rules(MobileData.ALL), mobile, 0))
  }

  @Test
  fun `the monthly cap stops mobile data, not wifi`() {
    val capped = rules(MobileData.ALL, cap = 2_000)
    assertEquals(null, NetworkRules.check("max", capped, mobile, 1_999))
    assertEquals(Wait.MOBILE_CAP, NetworkRules.check("max", capped, mobile, 2_000))
    assertEquals(null, NetworkRules.check("max", capped, wifi, 5_000))
  }

  @Test
  fun `charging only waits for the charger, on any network`() {
    val r = rules(MobileData.ALL, charging = true)
    assertEquals(Wait.CHARGING, NetworkRules.check("max", r, wifi, 0))
    assertEquals(null, NetworkRules.check("max", r, wifi.copy(charging = true), 0))
  }

  @Test
  fun `months follow the device's calendar`() {
    val paris = ZoneId.of("Europe/Paris")
    // 2026-10-31T23:30Z is already November in Paris.
    assertEquals("2026-11", NetworkRules.month(1_793_489_400_000, paris))
    assertEquals("2026-10", NetworkRules.month(1_793_489_400_000, ZoneId.of("UTC")))
  }
}
