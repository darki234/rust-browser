package dev.rustbrowser

import android.app.role.RoleManager
import android.content.ComponentCallbacks2
import android.content.Intent
import android.content.res.Configuration
import android.graphics.Color
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.view.View
import android.view.ViewGroup
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.widget.FrameLayout
import androidx.activity.ComponentActivity
import androidx.activity.OnBackPressedCallback
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import dev.rustbrowser.ui.BrowserApp

class MainActivity : ComponentActivity() {
    lateinit var store: Store
    lateinit var browser: Browser
    lateinit var sync: SyncManager
    lateinit var passwords: Passwords

    /** Тёмный ли сейчас интерфейс — по настройке или по теме системы. */
    val darkUi: Boolean
        get() = when (store.theme) {
            "light" -> false
            "dark", "amoled" -> true
            else -> (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES
        }
    val backdropColor: Int
        get() = when {
            store.theme == "amoled" && darkUi -> Color.BLACK
            darkUi -> Color.rgb(0x12, 0x13, 0x18)
            else -> Color.rgb(0xf3, 0xf4, 0xf9)
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        store = Store(filesDir)
        AdBlock.load(this)
        passwords = Passwords(filesDir)
        browser = Browser(this, store)
        sync = SyncManager(this, store, passwords)
        // вкладки, отправленные с компьютера, — открываются рядом, в фоне
        sync.onReceived = { list ->
            list.forEach { (url, title) -> browser.newTab(url, activate = false, title = title) }
            val text = if (list.size == 1) "Вкладка с компьютера: ${list[0].second.ifBlank { list[0].first }}" else "С компьютера пришло вкладок: ${list.size}"
            android.widget.Toast.makeText(this, text, android.widget.Toast.LENGTH_LONG).show()
        }
        applySystemBars()
        browser.restoreSession()
        handleIntent(intent)

        setContent { BrowserApp(browser, this) }

        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() = goBack()
        })
    }

    /** Строка состояния и жестовая панель — прозрачные, значки под цвет темы. */
    fun applySystemBars() {
        val dark = darkUi
        val style = if (dark) SystemBarStyle.dark(Color.TRANSPARENT) else SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT)
        enableEdgeToEdge(style, style)
        window.setBackgroundDrawable(android.graphics.drawable.ColorDrawable(backdropColor))
    }

    private fun goBack() {
        when {
            fullscreenView != null -> exitFullscreen()
            browser.screen == Screen.Find -> browser.findClose()
            browser.screen != Screen.None -> browser.screen = Screen.None
            browser.back() -> {}
            browser.active?.external == true -> {
                // пришли по ссылке из другого приложения — возвращаемся туда
                browser.active?.let { browser.closeTab(it) }
                moveTaskToBack(true)
            }
            browser.active?.isStart == false -> browser.goHome()
            else -> moveTaskToBack(true)
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handleIntent(intent)
    }

    private fun handleIntent(intent: Intent?) {
        intent ?: return
        when (intent.action) {
            Intent.ACTION_VIEW -> intent.dataString?.let { url ->
                browser.newTab().external = true
                browser.navigate(url)
            }
            Intent.ACTION_WEB_SEARCH -> intent.getStringExtra("query")?.let { browser.newTab(); browser.navigate(it) }
            Intent.ACTION_SEND -> intent.getStringExtra(Intent.EXTRA_TEXT)?.let { browser.newTab(); browser.navigate(it) }
        }
    }

    override fun onPause() {
        super.onPause()
        browser.saveSession()
        store.saveSettings()
        if (!isInPictureInPictureMode) browser.pauseAll()
        sync.syncIfStale(60_000) // изменения с телефона — в облако, пока не забыли
    }

    override fun onResume() {
        super.onResume()
        browser.resumeActive()
        sync.syncIfStale(5 * 60_000) // свежее с компьютера
    }

    @Suppress("DEPRECATION")
    override fun onTrimMemory(level: Int) {
        super.onTrimMemory(level)
        if (level >= ComponentCallbacks2.TRIM_MEMORY_RUNNING_LOW) browser.trimMemory()
    }

    // ---------- видео на весь экран ----------

    private var fullscreenView: View? = null
    private var fullscreenCallback: WebChromeClient.CustomViewCallback? = null

    fun enterFullscreen(view: View, callback: WebChromeClient.CustomViewCallback) {
        if (fullscreenView != null) { callback.onCustomViewHidden(); return }
        fullscreenView = view
        fullscreenCallback = callback
        view.setBackgroundColor(Color.BLACK)
        (window.decorView as FrameLayout).addView(view, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        WindowCompat.getInsetsController(window, view).apply {
            hide(WindowInsetsCompat.Type.systemBars())
            systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        }
    }

    fun exitFullscreen() {
        val view = fullscreenView ?: return
        (window.decorView as FrameLayout).removeView(view)
        WindowCompat.getInsetsController(window, window.decorView).show(WindowInsetsCompat.Type.systemBars())
        fullscreenCallback?.onCustomViewHidden()
        fullscreenView = null
        fullscreenCallback = null
    }

    // ---------- выбор файлов для сайтов ----------

    private var fileCallback: ValueCallback<Array<Uri>>? = null
    private val filePicker = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        fileCallback?.onReceiveValue(WebChromeClient.FileChooserParams.parseResult(result.resultCode, result.data))
        fileCallback = null
    }

    fun pickFiles(callback: ValueCallback<Array<Uri>>, params: WebChromeClient.FileChooserParams): Boolean {
        fileCallback?.onReceiveValue(null)
        fileCallback = callback
        return runCatching { filePicker.launch(params.createIntent()); true }.getOrElse { fileCallback = null; false }
    }

    // ---------- файл синхронизации в OneDrive ----------

    private var syncPicked: ((Uri) -> Unit)? = null
    private val syncPicker = registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri ?: return@registerForActivityResult
        // постоянный доступ: после перезапуска телефона файл открывается без выбора
        runCatching { contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION) }
            .onFailure { runCatching { contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION) } }
        syncPicked?.invoke(uri)
        syncPicked = null
    }

    fun pickSyncFile(done: (Uri) -> Unit) {
        syncPicked = done
        syncPicker.launch(arrayOf("*/*"))
    }

    fun fileName(uri: Uri): String = runCatching {
        contentResolver.query(uri, arrayOf(android.provider.OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
            if (c.moveToFirst()) c.getString(0) else null
        }
    }.getOrNull() ?: uri.lastPathSegment.orEmpty()

    // ---------- подтверждение личности (показ паролей) ----------

    private var authed = 0L
    private var afterAuth: (() -> Unit)? = null
    private val authLauncher = registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
        if (result.resultCode == RESULT_OK) { authed = System.currentTimeMillis(); afterAuth?.invoke() }
        afterAuth = null
    }

    /** Пароли показываются только после экрана блокировки телефона (на минуту). */
    fun authenticate(then: () -> Unit) {
        val keyguard = getSystemService(android.app.KeyguardManager::class.java)
        if (!keyguard.isDeviceSecure || System.currentTimeMillis() - authed < 60_000) return then()
        @Suppress("DEPRECATION")
        val intent = keyguard.createConfirmDeviceCredentialIntent("Пароли Rust Browser", "Подтвердите, что это вы") ?: return then()
        afterAuth = then
        authLauncher.launch(intent)
    }

    // ---------- браузер по умолчанию ----------

    fun requestDefaultBrowser() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val roles = getSystemService(RoleManager::class.java)
            if (roles.isRoleAvailable(RoleManager.ROLE_BROWSER) && !roles.isRoleHeld(RoleManager.ROLE_BROWSER)) {
                startActivity(roles.createRequestRoleIntent(RoleManager.ROLE_BROWSER))
                return
            }
        }
        startActivity(Intent(Settings.ACTION_MANAGE_DEFAULT_APPS_SETTINGS))
    }

    fun isDefaultBrowser(): Boolean =
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q && getSystemService(RoleManager::class.java).isRoleHeld(RoleManager.ROLE_BROWSER)
}
