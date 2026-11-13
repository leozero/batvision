package fr.batvision.sync

/** A subtitle of a plan item (WebVTT URL). */
data class PlanSubtitle(val url: String, val language: String?, val kind: String)

/** The fields of the server's `PlanItem` the device uses. */
data class PlanEntry(
  val kind: String,
  val id: Long,
  val profile: String,
  val audioLanguage: String?,
  val watched: Boolean,
  val status: String,
  val variantId: Long?,
  val progress: Double,
  val sizeBytes: Long?,
  val url: String?,
  val subtitles: List<PlanSubtitle>,
  val title: String,
  val episodeTitle: String? = null,
  val seriesId: Long? = null,
  val seasonNumber: Int? = null,
  val episodeNumber: Int? = null,
  val posterPath: String? = null,
  val durationMs: Long? = null,
  val error: String? = null,
  /** `manual`, `rolling`, `season` or `watchlist`: what cancelling removes (D74). */
  val source: String? = null,
) {
  val key get() = itemKey(kind, id)
}

/** What is on disk for an item. */
data class LocalRecord(
  val key: String,
  val complete: Boolean,
  /** The variant the file (or partial file) comes from. */
  val variantId: Long?,
  val profile: String,
  val audioLanguage: String?,
  /** Bytes on disk. */
  val sizeBytes: Long,
)

sealed interface Action {
  val key: String

  data class Request(override val key: String, val kind: String, val id: Long) : Action

  data class Download(override val key: String, val entry: PlanEntry) : Action

  data class Confirm(override val key: String, val variantId: Long) : Action
}

data class Decision(
  /** Requests and confirmations to send, then downloads in priority order. */
  val actions: List<Action>,
  /** Local items to delete (no longer expected, over quota, or replaced). */
  val remove: List<String>,
  /** Expected items that don't fit in the quota. */
  val overQuota: Set<String>,
  /** Space the kept items take (or will take once downloaded). */
  val usedBytes: Long,
)

fun itemKey(kind: String, id: Long) = "$kind-$id"

/**
 * What the device does with the server's expected set. Same rules as the desktop app
 * (apps/desktop/src/main/planner.ts): the server lists items in priority order; keep the
 * longest prefix that fits the quota, delete the rest. An item not encoded yet has no
 * known size: it's requested only while there is room left.
 */
object Planner {
  private fun matches(local: LocalRecord, entry: PlanEntry) =
    local.profile == entry.profile && local.audioLanguage == entry.audioLanguage

  /** The first download still to do, in the plan's order; `skip` says which keys are settled. */
  fun nextDownload(actions: List<Action>, skip: (String) -> Boolean): Action.Download? =
    actions.firstOrNull { it is Action.Download && !skip(it.key) } as Action.Download?

  fun decide(plan: List<PlanEntry>, local: Map<String, LocalRecord>, quotaBytes: Long?): Decision {
    val quota = quotaBytes ?: Long.MAX_VALUE
    val requests = mutableListOf<Action>()
    val downloads = mutableListOf<Action>()
    val kept = mutableSetOf<String>()
    val overQuota = linkedSetOf<String>()
    var used = 0L
    var full = false

    for (entry in plan) {
      val key = entry.key
      val have = local[key]
      val complete = have?.takeIf { it.complete && matches(it, entry) }
      val size = complete?.sizeBytes ?: entry.sizeBytes
      if (full || (size != null && used + size > quota)) {
        full = true
        overQuota += key
        continue
      }

      if (complete != null) {
        kept += key
        used += complete.sizeBytes
        // The server forgot we have it (or the confirmation got lost).
        if (entry.status == "ready" && complete.variantId != null && entry.variantId == complete.variantId) {
          requests += Action.Confirm(key, complete.variantId)
        }
        continue
      }

      when (entry.status) {
        "ready" ->
          if (entry.url != null && size != null) {
            kept += key
            used += size
            downloads += Action.Download(key, entry)
          }
        "queued", "encoding" -> if (used < quota) kept += key
        // Confirmed earlier but missing here (deleted by hand, or another profile).
        "not_requested", "downloaded" ->
          if (used < quota) {
            requests += Action.Request(key, entry.kind, entry.id)
            kept += key
          }
        // failed or unavailable: nothing to do until the server changes its mind.
        else -> {}
      }
    }

    val remove = local.values.filter { l ->
      if (l.key !in kept) return@filter true
      val entry = plan.firstOrNull { it.key == l.key } ?: return@filter true
      // A stale copy (other profile or audio) or a partial file of another variant.
      if (l.complete) !matches(l, entry) else l.variantId != entry.variantId
    }.map { it.key }

    return Decision(requests + downloads, remove, overQuota, used)
  }
}
