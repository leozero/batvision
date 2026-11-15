// React Native takes the screen density from the phone's main display (the application
// context's default display). On Samsung DeX or AR glasses the app runs on another display
// with another density: texts were laid out with one and drawn with the other, cut in half
// (D105). MainActivity gives it the metrics of the display it actually runs on.
const { withMainActivity } = require("expo/config-plugins");

const MARK = "BatVision: display metrics";

const METHODS = `
  // ${MARK} (D105): React Native reads the phone's main display; on Samsung DeX or
  // glasses the app runs on another one. Its own display's metrics, set again after React
  // Native's own (it resets them on layout changes).
  private var displayMetricsListening = false

  private fun useOwnDisplayMetrics() {
    val own = resources.displayMetrics
    val metrics = android.util.DisplayMetrics()
    metrics.setTo(own)
    try {
      @Suppress("DEPRECATION")
      val shown = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) display else windowManager.defaultDisplay
      @Suppress("DEPRECATION")
      shown?.getRealMetrics(metrics)
    } catch (_: Exception) {
    }
    metrics.density = own.density
    metrics.densityDpi = own.densityDpi
    @Suppress("DEPRECATION")
    metrics.scaledDensity = own.scaledDensity
    metrics.xdpi = own.xdpi
    metrics.ydpi = own.ydpi
    com.facebook.react.uimanager.DisplayMetricsHolder.setScreenDisplayMetrics(metrics)
  }

  override fun onResume() {
    super.onResume()
    useOwnDisplayMetrics()
    if (!displayMetricsListening) {
      displayMetricsListening = true
      // Posted: registered after React Native's own listener, so it runs after it.
      window.decorView.post {
        window.decorView.viewTreeObserver.addOnGlobalLayoutListener { useOwnDisplayMetrics() }
      }
    }
  }

  override fun onConfigurationChanged(newConfig: android.content.res.Configuration) {
    super.onConfigurationChanged(newConfig)
    useOwnDisplayMetrics()
  }
`;

module.exports = function withDisplayMetrics(config) {
  return withMainActivity(config, (mod) => {
    let src = mod.modResults.contents;
    if (!src.includes(MARK)) {
      src = src.replace(
        /super\.onCreate\(null\)\n/,
        "super.onCreate(null)\n    useOwnDisplayMetrics()\n",
      );
      // Before the class's last brace.
      const end = src.lastIndexOf("}");
      src = `${src.slice(0, end)}${METHODS}}\n`;
      mod.modResults.contents = src;
    }
    return mod;
  });
};
