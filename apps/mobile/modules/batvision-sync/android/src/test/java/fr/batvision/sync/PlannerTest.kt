package fr.batvision.sync

import org.junit.Assert.assertEquals
import org.junit.Test

/** Same cases as the desktop planner (apps/desktop/src/main/planner.test.ts). */
class PlannerTest {
  private val gb = 1_000_000_000L

  private fun entry(id: Long, status: String, profile: String = "max", sizeBytes: Long? = if (status == "ready") gb else null) =
    PlanEntry(
      kind = "episode",
      id = id,
      profile = profile,
      audioLanguage = null,
      watched = false,
      status = status,
      variantId = if (status == "not_requested") null else id * 10,
      progress = 0.0,
      sizeBytes = sizeBytes,
      url = if (status == "ready") "/api/downloads/file/t$id" else null,
      subtitles = emptyList(),
      title = "Show",
    )

  private fun local(
    id: Long,
    complete: Boolean = true,
    variantId: Long? = id * 10,
    profile: String = "max",
    sizeBytes: Long = gb,
  ) = "episode-$id" to LocalRecord("episode-$id", complete, variantId, profile, null, sizeBytes)

  private fun Decision.steps() = actions.map {
    when (it) {
      is Action.Request -> "request:${it.key}"
      is Action.Download -> "download:${it.key}"
      is Action.Confirm -> "confirm:${it.key}"
    }
  }

  @Test
  fun `requests, waits for and downloads expected items in order`() {
    val d = Planner.decide(listOf(entry(1, "ready"), entry(2, "encoding"), entry(3, "not_requested")), emptyMap(), null)
    assertEquals(listOf("request:episode-3", "download:episode-1"), d.steps())
    assertEquals(emptyList<String>(), d.remove)
  }

  @Test
  fun `keeps what it has and deletes what is no longer expected`() {
    val d = Planner.decide(listOf(entry(1, "downloaded")), linkedMapOf(local(1), local(2)), null)
    assertEquals(emptyList<String>(), d.steps())
    assertEquals(listOf("episode-2"), d.remove)
    assertEquals(gb, d.usedBytes)
  }

  @Test
  fun `keeps the longest prefix that fits the quota`() {
    val d = Planner.decide(
      listOf(entry(1, "downloaded"), entry(2, "ready"), entry(3, "downloaded")),
      linkedMapOf(local(1), local(3)),
      gb * 5 / 2,
    )
    // 1 and 2 fit (2 GB); 3 would make 3 GB: deleted, even though it's on disk.
    assertEquals(listOf("download:episode-2"), d.steps())
    assertEquals(listOf("episode-3"), d.remove)
    assertEquals(setOf("episode-3"), d.overQuota)
  }

  @Test
  fun `stops at the first item that doesn't fit, even if a later one would`() {
    val d = Planner.decide(listOf(entry(1, "ready", sizeBytes = 3 * gb), entry(2, "ready")), emptyMap(), 2 * gb)
    assertEquals(emptyList<String>(), d.steps())
    assertEquals(setOf("episode-1", "episode-2"), d.overQuota)
  }

  @Test
  fun `requests items of unknown size only while there is room`() {
    val d = Planner.decide(listOf(entry(1, "downloaded"), entry(2, "not_requested")), linkedMapOf(local(1)), gb)
    assertEquals(emptyList<String>(), d.steps())
  }

  @Test
  fun `requests again an item confirmed earlier but missing on disk`() {
    val d = Planner.decide(listOf(entry(1, "downloaded")), emptyMap(), null)
    assertEquals(listOf(Action.Request("episode-1", "episode", 1)), d.actions)
  }

  @Test
  fun `confirms a complete copy the server still offers`() {
    val d = Planner.decide(listOf(entry(1, "ready")), linkedMapOf(local(1)), null)
    assertEquals(listOf(Action.Confirm("episode-1", 10)), d.actions)
  }

  @Test
  fun `replaces a copy made with another profile`() {
    val d = Planner.decide(listOf(entry(1, "ready", profile = "mobile")), linkedMapOf(local(1, profile = "max")), null)
    assertEquals(listOf("episode-1"), d.remove)
    assertEquals(listOf("download:episode-1"), d.steps())
  }

  @Test
  fun `keeps a copy when only the audio language changed`() {
    val (key, record) = local(1)
    val d = Planner.decide(
      listOf(entry(1, "downloaded").copy(audioLanguage = "fre")),
      linkedMapOf(key to record.copy(audioLanguage = "eng")),
      null,
    )
    assertEquals(emptyList<String>(), d.remove)
    assertEquals(emptyList<String>(), d.steps())
  }

  @Test
  fun `drops a partial file from another variant, keeps one from the same`() {
    val same = Planner.decide(listOf(entry(1, "ready")), linkedMapOf(local(1, complete = false, sizeBytes = 100)), null)
    assertEquals(emptyList<String>(), same.remove)
    val other = Planner.decide(
      listOf(entry(1, "ready")),
      linkedMapOf(local(1, complete = false, variantId = 99, sizeBytes = 100)),
      null,
    )
    assertEquals(listOf("episode-1"), other.remove)
  }

  @Test
  fun `ignores failed and unavailable items`() {
    val d = Planner.decide(listOf(entry(1, "failed"), entry(2, "unavailable")), emptyMap(), null)
    assertEquals(emptyList<String>(), d.steps())
  }

  @Test
  fun `the next download skips settled items and follows a changed decision`() {
    val plan = listOf(entry(1, "ready"), entry(2, "ready"), entry(3, "not_requested"), entry(4, "ready"))
    val d = Planner.decide(plan, emptyMap(), null)
    assertEquals("episode-1", Planner.nextDownload(d.actions) { false }?.key)
    // Done, failed or waiting items are passed over, in the plan's order.
    assertEquals("episode-4", Planner.nextDownload(d.actions) { it in setOf("episode-1", "episode-2") }?.key)
    assertEquals(null, Planner.nextDownload(d.actions) { true })
    // Dropped by hand (D74): the decision made again no longer offers it.
    val after = Planner.decide(plan.filter { it.id != 1L }, emptyMap(), null)
    assertEquals("episode-2", Planner.nextDownload(after.actions) { false }?.key)
    assertEquals(null, Planner.nextDownload(emptyList()) { false })
  }
}
