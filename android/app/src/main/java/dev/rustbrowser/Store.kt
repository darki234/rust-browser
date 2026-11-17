package dev.rustbrowser

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.concurrent.Executors

data class Bookmark(val url: String, val title: String)
data class Visit(val url: String, val title: String, val time: Long)
data class SavedTab(val url: String, val title: String)

val DEFAULT_BOOKMARKS = listOf(
    Bookmark("https://www.youtube.com/", "YouTube"),
    Bookmark("https://ya.ru/", "Яндекс"),
    Bookmark("https://ru.wikipedia.org/", "Википедия"),
    Bookmark("https://github.com/", "GitHub"),
)

/** Поисковые системы — те же, что у настольной версии. */
val ENGINES = listOf(
    Triple("google", "Google", "https://www.google.com/search?q="),
    Triple("yandex", "Яндекс", "https://yandex.ru/search/?text="),
    Triple("bing", "Bing", "https://www.bing.com/search?q="),
    Triple("duckduckgo", "DuckDuckGo", "https://duckduckgo.com/?q="),
)

/**
 * Настройки, закладки, история и сеанс — JSON-файлы в папке приложения.
 * Пишутся в отдельном потоке: интерфейс никогда не ждёт диска.
 */
class Store(private val dir: File) {
    private val io = Executors.newSingleThreadExecutor()

    // ---------- настройки (Compose следит за ними и перерисовывает экран) ----------
    var theme by mutableStateOf("auto")          // auto | light | dark | amoled
    var engine by mutableStateOf("google")
    var adblock by mutableStateOf(true)
    var darkSites by mutableStateOf(true)        // тёмная тема для сайтов силами движка
    var accent by mutableStateOf(0)
    var blockedTotal by mutableStateOf(0L)
    /** Когда историю очистили (секунды Unix) — как history_cleared у компьютера: старое не вернётся из облака. */
    var historyCleared = 0L

    val bookmarks = mutableStateListOf<Bookmark>()
    val history = mutableStateListOf<Visit>()

    init {
        read("settings.json")?.let { s ->
            theme = s.optString("theme", theme)
            engine = s.optString("engine", engine)
            adblock = s.optBoolean("adblock", adblock)
            darkSites = s.optBoolean("darkSites", darkSites)
            accent = s.optInt("accent", accent)
            blockedTotal = s.optLong("blockedTotal", 0)
            historyCleared = s.optLong("historyCleared", 0)
        }
        readArray("bookmarks.json")?.let { a ->
            for (i in 0 until a.length()) a.optJSONObject(i)?.let { bookmarks += Bookmark(it.optString("url"), it.optString("title")) }
        }
        if (!File(dir, "bookmarks.json").exists()) bookmarks += DEFAULT_BOOKMARKS
        readArray("history.json")?.let { a ->
            for (i in 0 until a.length()) a.optJSONObject(i)?.let { history += Visit(it.optString("url"), it.optString("title"), it.optLong("time")) }
        }
    }

    fun searchUrl(query: String): String {
        val base = ENGINES.firstOrNull { it.first == engine }?.third ?: ENGINES[0].third
        return base + java.net.URLEncoder.encode(query, "UTF-8")
    }

    fun saveSettings() {
        val json = JSONObject()
            .put("theme", theme).put("engine", engine).put("adblock", adblock)
            .put("darkSites", darkSites).put("accent", accent).put("blockedTotal", blockedTotal)
            .put("historyCleared", historyCleared)
        write("settings.json", json.toString())
    }

    /** Плитки остались те, что телефон поставил сам, — пользователь их не трогал. */
    fun bookmarksAreDefaults() = bookmarks.toList() == DEFAULT_BOOKMARKS

    fun replaceBookmarks(list: List<Bookmark>) {
        bookmarks.clear(); bookmarks.addAll(list); saveBookmarks()
    }

    fun replaceHistory(list: List<Visit>, cleared: Long) {
        history.clear(); history.addAll(list.take(5000))
        historyCleared = maxOf(historyCleared, cleared)
        saveSettings(); saveHistorySoon()
    }

    fun isBookmarked(url: String) = bookmarks.any { it.url == url }

    fun toggleBookmark(url: String, title: String) {
        val i = bookmarks.indexOfFirst { it.url == url }
        if (i >= 0) bookmarks.removeAt(i) else bookmarks += Bookmark(url, title.ifBlank { url })
        saveBookmarks()
    }

    fun removeBookmark(b: Bookmark) { bookmarks.remove(b); saveBookmarks() }

    private fun saveBookmarks() {
        val a = JSONArray()
        bookmarks.forEach { a.put(JSONObject().put("url", it.url).put("title", it.title)) }
        write("bookmarks.json", a.toString())
    }

    /** Посещение: тот же адрес подряд не дублируется, журнал — до 3000 записей. */
    fun addVisit(url: String, title: String) {
        if (!url.startsWith("http")) return
        val last = history.firstOrNull()
        if (last?.url == url) {
            if (title.isNotBlank() && last.title != title) history[0] = last.copy(title = title)
        } else {
            history.add(0, Visit(url, title, System.currentTimeMillis()))
            if (history.size > 3000) history.removeRange(3000, history.size)
        }
        saveHistorySoon()
    }

    fun clearHistory() {
        history.clear(); write("history.json", "[]")
        historyCleared = System.currentTimeMillis() / 1000 // на компьютерах она тоже очистится
        saveSettings()
    }

    private val main = android.os.Handler(android.os.Looper.getMainLooper())
    private var historyQueued = false

    /** Журнал пишется пачкой через 3 секунды после перехода, а не на каждый переход. */
    private fun saveHistorySoon() {
        if (historyQueued) return
        historyQueued = true
        main.postDelayed({
            historyQueued = false
            val a = JSONArray()
            history.forEach { a.put(JSONObject().put("url", it.url).put("title", it.title).put("time", it.time)) }
            write("history.json", a.toString())
        }, 3000)
    }

    // ---------- сеанс: открытые вкладки ----------
    fun saveSession(tabs: List<SavedTab>, active: Int) {
        val a = JSONArray()
        tabs.forEach { a.put(JSONObject().put("url", it.url).put("title", it.title)) }
        write("session.json", JSONObject().put("tabs", a).put("active", active).toString())
    }

    fun loadSession(): Pair<List<SavedTab>, Int> {
        val s = read("session.json") ?: return emptyList<SavedTab>() to 0
        val a = s.optJSONArray("tabs") ?: JSONArray()
        val tabs = (0 until a.length()).mapNotNull { a.optJSONObject(it) }.map { SavedTab(it.optString("url"), it.optString("title")) }
        return tabs to s.optInt("active", 0)
    }

    private fun read(name: String): JSONObject? = runCatching { JSONObject(File(dir, name).readText()) }.getOrNull()
    private fun readArray(name: String): JSONArray? = runCatching { JSONArray(File(dir, name).readText()) }.getOrNull()
    private fun write(name: String, text: String) = io.execute {
        // сначала во временный файл: оборвавшаяся запись не испортит старый
        val tmp = File(dir, "$name.tmp")
        tmp.writeText(text)
        tmp.renameTo(File(dir, name))
    }
}
