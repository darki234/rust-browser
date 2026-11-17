package dev.rustbrowser

import android.annotation.SuppressLint
import android.app.DownloadManager
import android.content.ActivityNotFoundException
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.net.Uri
import android.os.Environment
import android.os.Handler
import android.os.Looper
import android.os.Message
import android.view.ContextThemeWrapper
import android.view.View
import android.view.ViewGroup
import android.webkit.CookieManager
import android.webkit.GeolocationPermissions
import android.webkit.PermissionRequest
import android.webkit.URLUtil
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebStorage
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import android.widget.Toast
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.webkit.ProfileStore
import androidx.webkit.WebSettingsCompat
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature

enum class Screen { None, Search, Tabs, Menu, Bookmarks, History, Settings, Find }

/** Вкладка. WebView создаётся только когда вкладку впервые открыли — запуск мгновенный. */
class Tab(val id: Int, url: String, title: String, val incognito: Boolean = false) {
    var url by mutableStateOf(url)
    var title by mutableStateOf(title)
    var progress by mutableIntStateOf(100)
    var loading by mutableStateOf(false)
    var canBack by mutableStateOf(false)
    var canForward by mutableStateOf(false)
    var favicon by mutableStateOf<ImageBitmap?>(null)
    var thumbnail by mutableStateOf<ImageBitmap?>(null)
    var desktop by mutableStateOf(false)
    var blocked by mutableIntStateOf(0)
    var webView: WebView? = null
    /** Открыта по ссылке из другого приложения: «назад» на первой странице вернёт туда. */
    var external = false
    val isStart get() = url.isEmpty()
}

private const val INCOGNITO_PROFILE = "incognito"
private const val DESKTOP_UA =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"

class Browser(private val activity: MainActivity, val store: Store) {
    val tabs = mutableStateListOf<Tab>()
    var activeId by mutableIntStateOf(-1)
    var screen by mutableStateOf(Screen.None)
    var islandShown by mutableStateOf(true)
    var showingIncognito by mutableStateOf(false)
    var findCount by mutableStateOf(0 to 0)
    /** Сюда вставляется WebView активной вкладки. */
    val host = FrameLayout(activity)
    private var nextId = 1
    private val main = Handler(Looper.getMainLooper())

    val active: Tab? get() = tabs.firstOrNull { it.id == activeId }

    // ---------- вкладки ----------

    fun newTab(url: String = "", incognito: Boolean = false, activate: Boolean = true, title: String = ""): Tab {
        val tab = Tab(nextId++, url, title, incognito)
        val index = tabs.indexOfFirst { it.id == activeId }
        if (index >= 0 && activate) tabs.add(index + 1, tab) else tabs.add(tab)
        if (activate) activate(tab)
        saveSessionSoon()
        return tab
    }

    fun activate(tab: Tab) {
        active?.let { if (it.id != tab.id) capture(it) }
        activeId = tab.id
        islandShown = true
        saveSessionSoon()
        attach(tab)
    }

    /** Показывает WebView вкладки; остальные ставит на паузу — они не тратят процессор. */
    private fun attach(tab: Tab) {
        if (tab.webView == null && !tab.isStart) {
            tab.webView = createWebView(tab).also { it.loadUrl(tab.url) }
        }
        host.removeAllViews()
        tabs.forEach { if (it.id != tab.id) it.webView?.onPause() }
        tab.webView?.let { wv ->
            (wv.parent as? ViewGroup)?.removeView(wv)
            host.addView(wv, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
            wv.onResume()
        }
    }

    fun closeTab(tab: Tab) {
        val index = tabs.indexOf(tab)
        if (index < 0) return
        tabs.removeAt(index)
        tab.webView?.let { host.removeView(it); it.destroy() }
        tab.webView = null
        if (tab.incognito && tabs.none { it.incognito }) dropIncognito()
        saveSessionSoon()
        if (tab.id == activeId) {
            val next = tabs.getOrNull(index) ?: tabs.getOrNull(index - 1)
            if (next != null) activate(next) else newTab()
        }
    }

    fun closeAll(incognito: Boolean) {
        tabs.filter { it.incognito == incognito }.forEach { closeTab(it) }
    }

    /** Вперёд/назад по вкладкам жестом по острову. */
    fun switchBy(step: Int) {
        val list = tabs.filter { it.incognito == (active?.incognito ?: false) }
        val i = list.indexOfFirst { it.id == activeId }
        list.getOrNull(i + step)?.let { activate(it) }
    }

    // ---------- переходы ----------

    /** Адрес или поисковый запрос — как в адресной строке настольной версии. */
    fun navigate(text: String, tab: Tab? = active) {
        val input = text.trim()
        if (input.isEmpty()) return
        val url = when {
            Regex("^[a-zA-Z][a-zA-Z0-9+.-]*://").containsMatchIn(input) || input.startsWith("about:") -> input
            !input.contains(' ') && (Regex("^[^/\\s]+\\.[a-zA-Zа-яА-Я]{2,}(:\\d+)?(/.*)?$").matches(input) || input.startsWith("localhost")) -> "https://$input"
            else -> store.searchUrl(input)
        }
        val target = tab ?: newTab()
        target.url = url
        screen = Screen.None
        if (target.webView == null) {
            target.webView = createWebView(target)
            if (target.id == activeId) attach(target)
        }
        target.webView?.loadUrl(url)
    }

    fun back(): Boolean {
        val wv = active?.webView ?: return false
        if (wv.canGoBack()) { wv.goBack(); return true }
        return false
    }

    fun forward() { active?.webView?.takeIf { it.canGoForward() }?.goForward() }

    fun reloadOrStop() {
        val tab = active ?: return
        if (tab.loading) tab.webView?.stopLoading() else tab.webView?.reload()
    }

    fun goHome() {
        val tab = active ?: return
        tab.webView?.let { host.removeView(it); it.destroy() }
        tab.webView = null
        tab.url = ""; tab.title = ""; tab.favicon = null; tab.loading = false
        screen = Screen.None
    }

    fun toggleDesktop() {
        val tab = active ?: return
        tab.desktop = !tab.desktop
        tab.webView?.let { applyDesktop(it, tab.desktop); it.reload() }
    }

    private fun applyDesktop(wv: WebView, on: Boolean) {
        wv.settings.userAgentString = if (on) DESKTOP_UA else null
        wv.settings.useWideViewPort = true
        wv.settings.loadWithOverviewMode = true
    }

    fun find(query: String) { active?.webView?.findAllAsync(query) }
    fun findNext(forward: Boolean) { active?.webView?.findNext(forward) }
    fun findClose() { active?.webView?.clearMatches(); findCount = 0 to 0; screen = Screen.None }

    fun share() {
        val tab = active ?: return
        if (tab.isStart) return
        val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, tab.url).putExtra(Intent.EXTRA_SUBJECT, tab.title)
        activity.startActivity(Intent.createChooser(send, null))
    }

    // ---------- тёмные сайты, память, сеанс ----------

    /**
     * Тема интерфейса сменилась. WebView узнаёт светлый или тёмный режим только при
     * создании, поэтому вкладки пересоздаются, а их история переносится (saveState).
     */
    private var themedDark: Boolean? = null
    fun retheme(dark: Boolean) {
        val was = themedDark
        themedDark = dark
        if (was == null || was == dark) return
        tabs.forEach { tab ->
            val old = tab.webView ?: return@forEach
            val state = android.os.Bundle()
            old.saveState(state)
            host.removeView(old)
            old.destroy()
            val fresh = createWebView(tab)
            tab.webView = fresh
            if (fresh.restoreState(state) == null) fresh.loadUrl(tab.url)
        }
        active?.let { attach(it) }
    }

    fun applyDarkSites() {
        if (!WebViewFeature.isFeatureSupported(WebViewFeature.ALGORITHMIC_DARKENING)) return
        tabs.forEach { t -> t.webView?.let { WebSettingsCompat.setAlgorithmicDarkeningAllowed(it.settings, store.darkSites) } }
    }

    /** Системе мало памяти — фоновые вкладки выгружаются целиком, адреса остаются. */
    fun trimMemory() {
        tabs.filter { it.id != activeId }.forEach { t ->
            t.webView?.let { capture(t); host.removeView(it); it.destroy() }
            t.webView = null
        }
    }

    /** Приложение ушло в фон: таймеры JS останавливаются во всех вкладках сразу. */
    fun pauseAll() {
        tabs.forEach { it.webView?.onPause() }
        tabs.firstNotNullOfOrNull { it.webView }?.pauseTimers()
        CookieManager.getInstance().flush()
    }
    fun resumeActive() { active?.webView?.let { it.resumeTimers(); it.onResume() } }

    /** Сеанс пишется вскоре после любого изменения: убитое системой приложение не теряет вкладки. */
    private val saveTask = Runnable { saveSession() }
    fun saveSessionSoon() {
        main.removeCallbacks(saveTask)
        main.postDelayed(saveTask, 1000)
    }

    fun saveSession() {
        val normal = tabs.filter { !it.incognito }
        val active = normal.indexOfFirst { it.id == activeId }.coerceAtLeast(0)
        store.saveSession(normal.map { SavedTab(it.url, it.title) }, active)
    }

    fun restoreSession() {
        val (saved, active) = store.loadSession()
        saved.forEach { tabs += Tab(nextId++, it.url, it.title) }
        if (tabs.isEmpty()) newTab() else activate(tabs[active.coerceIn(0, tabs.lastIndex)])
    }

    fun clearData() {
        CookieManager.getInstance().removeAllCookies(null)
        WebStorage.getInstance().deleteAllData()
        tabs.forEach { it.webView?.clearCache(true) }
        Toast.makeText(activity, "Cookie и данные сайтов удалены", Toast.LENGTH_SHORT).show()
    }

    /** Снимок вкладки для сетки вкладок (уменьшенный — экономит память). */
    fun capture(tab: Tab) {
        val wv = tab.webView ?: return
        if (wv.width == 0 || wv.height == 0) return
        val scale = 0.33f
        runCatching {
            val bmp = Bitmap.createBitmap((wv.width * scale).toInt(), (wv.height * scale).toInt(), Bitmap.Config.RGB_565)
            val canvas = Canvas(bmp)
            canvas.scale(scale, scale)
            canvas.translate(-wv.scrollX.toFloat(), -wv.scrollY.toFloat())
            wv.draw(canvas)
            tab.thumbnail = bmp.asImageBitmap()
        }
    }

    // ---------- WebView ----------

    @SuppressLint("SetJavaScriptEnabled")
    private fun createWebView(tab: Tab): WebView {
        // тема контекста — от неё зависит prefers-color-scheme у сайтов
        val ctx = ContextThemeWrapper(activity, if (activity.darkUi) R.style.Page_Dark else R.style.Page_Light)
        val wv = WebView(ctx)
        if (tab.incognito && WebViewFeature.isFeatureSupported(WebViewFeature.MULTI_PROFILE)) {
            ProfileStore.getInstance().getOrCreateProfile(INCOGNITO_PROFILE)
            WebViewCompat.setProfile(wv, INCOGNITO_PROFILE)
        }
        wv.settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = true
            loadWithOverviewMode = true
            useWideViewPort = true
            builtInZoomControls = true
            displayZoomControls = false
            setSupportMultipleWindows(true)
            javaScriptCanOpenWindowsAutomatically = false
            mixedContentMode = WebSettings.MIXED_CONTENT_COMPATIBILITY_MODE
            mediaPlaybackRequiresUserGesture = false
            if (tab.incognito) cacheMode = WebSettings.LOAD_NO_CACHE
        }
        if (WebViewFeature.isFeatureSupported(WebViewFeature.ALGORITHMIC_DARKENING)) {
            WebSettingsCompat.setAlgorithmicDarkeningAllowed(wv.settings, store.darkSites)
        }
        if (tab.desktop) applyDesktop(wv, true)
        wv.setBackgroundColor(activity.backdropColor)
        CookieManager.getInstance().setAcceptThirdPartyCookies(wv, !tab.incognito)
        wv.webViewClient = PageClient(tab)
        wv.webChromeClient = ChromeClient(tab)
        wv.setDownloadListener { url, userAgent, disposition, mime, _ -> download(url, userAgent, disposition, mime, tab) }
        wv.setFindListener { active, total, done -> if (done) findCount = (if (total == 0) 0 else active + 1) to total }
        wv.setOnScrollChangeListener { _, _, y, _, oldY -> onScroll(tab, y, oldY) }
        return wv
    }

    private fun dropIncognito() {
        if (!WebViewFeature.isFeatureSupported(WebViewFeature.MULTI_PROFILE)) return
        // все вкладки инкогнито закрыты — его cookie и данные стираются
        main.post { runCatching { ProfileStore.getInstance().deleteProfile(INCOGNITO_PROFILE) } }
    }

    /** Прокрутка вниз прячет остров, вверх — возвращает (как в настольной версии). */
    private var scrollAcc = 0
    private fun onScroll(tab: Tab, y: Int, oldY: Int) {
        if (tab.id != activeId) return
        val dy = y - oldY
        scrollAcc = if ((dy > 0) == (scrollAcc > 0)) scrollAcc + dy else dy
        if (y < 40) islandShown = true
        else if (scrollAcc > 40) islandShown = false
        else if (scrollAcc < -40) islandShown = true
    }

    private fun download(url: String, userAgent: String, disposition: String?, mime: String?, tab: Tab) {
        val name = URLUtil.guessFileName(url, disposition, mime)
        runCatching {
            val request = DownloadManager.Request(Uri.parse(url))
                .setMimeType(mime)
                .addRequestHeader("User-Agent", userAgent)
                .setTitle(name)
                .setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
                .setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, name)
            CookieManager.getInstance().getCookie(url)?.let { request.addRequestHeader("Cookie", it) }
            (activity.getSystemService(DownloadManager::class.java)).enqueue(request)
            Toast.makeText(activity, "Загрузка: $name", Toast.LENGTH_SHORT).show()
        }.onFailure { Toast.makeText(activity, "Не удалось скачать файл", Toast.LENGTH_SHORT).show() }
        // ссылка на файл открылась в новом окне — пустая вкладка не нужна
        if (tab.url == "about:blank" && tabs.size > 1) closeTab(tab)
    }

    private inner class PageClient(private val tab: Tab) : WebViewClient() {
        override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
            if (!store.adblock || request.isForMainFrame || !AdBlock.isBlocked(request.url)) return null
            main.post { tab.blocked++; store.blockedTotal++ }
            return AdBlock.emptyResponse()
        }

        override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
            val uri = request.url
            when (uri.scheme) {
                "http", "https", "about", "data", "javascript", "blob", "file" -> return false
            }
            // tel:, mailto:, intent://, market:// — в своё приложение
            return runCatching {
                val intent = if (uri.scheme == "intent") Intent.parseUri(uri.toString(), Intent.URI_INTENT_SCHEME) else Intent(Intent.ACTION_VIEW, uri)
                intent.addCategory(Intent.CATEGORY_BROWSABLE).setComponent(null).setSelector(null)
                try {
                    activity.startActivity(intent)
                } catch (_: ActivityNotFoundException) {
                    intent.getStringExtra("browser_fallback_url")?.let { view.loadUrl(it) }
                }
                true
            }.getOrDefault(true)
        }

        override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) {
            tab.loading = true
            tab.url = url
            tab.blocked = 0
            if (tab.id == activeId) islandShown = true
        }

        override fun onPageFinished(view: WebView, url: String) {
            tab.loading = false
            tab.canBack = view.canGoBack()
            tab.canForward = view.canGoForward()
            if (!tab.incognito) { store.addVisit(url, view.title ?: ""); saveSessionSoon() }
        }

        override fun doUpdateVisitedHistory(view: WebView, url: String, isReload: Boolean) {
            tab.url = url
            tab.canBack = view.canGoBack()
            tab.canForward = view.canGoForward()
        }
    }

    private inner class ChromeClient(private val tab: Tab) : WebChromeClient() {
        override fun onProgressChanged(view: WebView, newProgress: Int) {
            tab.progress = newProgress
            if (newProgress >= 100) tab.loading = false
        }

        override fun onReceivedTitle(view: WebView, title: String?) { tab.title = title.orEmpty() }

        override fun onReceivedIcon(view: WebView, icon: Bitmap?) {
            icon ?: return
            tab.favicon = icon.asImageBitmap()
            // значок сайта запоминается — плитки на стартовой странице берут его без сети
            Uri.parse(tab.url).host?.let { SiteIcons.remember(activity, it, icon) }
        }

        override fun onCreateWindow(view: WebView, isDialog: Boolean, isUserGesture: Boolean, resultMsg: Message): Boolean {
            if (!isUserGesture) return false // всплывающие окна без нажатия — блокируем
            val child = newTab(incognito = tab.incognito, activate = true)
            child.url = "about:blank"
            val wv = createWebView(child)
            child.webView = wv
            attach(child)
            (resultMsg.obj as WebView.WebViewTransport).webView = wv
            resultMsg.sendToTarget()
            return true
        }

        override fun onCloseWindow(window: WebView) { tabs.firstOrNull { it.webView == window }?.let { closeTab(it) } }

        override fun onShowCustomView(view: View, callback: CustomViewCallback) = activity.enterFullscreen(view, callback)
        override fun onHideCustomView() = activity.exitFullscreen()

        override fun onShowFileChooser(view: WebView, callback: ValueCallback<Array<Uri>>, params: FileChooserParams): Boolean =
            activity.pickFiles(callback, params)

        override fun onGeolocationPermissionsShowPrompt(origin: String, callback: GeolocationPermissions.Callback) {
            callback.invoke(origin, false, false)
        }

        override fun onPermissionRequest(request: PermissionRequest) { request.deny() }
    }
}
