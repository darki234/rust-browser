package dev.rustbrowser

import android.content.Context
import android.net.Uri
import android.os.Handler
import android.os.Looper
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.concurrent.Executors

/**
 * Вход в аккаунт и синхронизация. Тяжёлое (ключ из пароля, шифр, чтение облака) — в
 * фоновом потоке; на главном только снимок данных и применение результата.
 */
class SyncManager(private val context: Context, private val store: Store) {
    private val dir: File = context.filesDir
    private val engine = SyncEngine(context, store)
    private val worker = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())

    var account by mutableStateOf(SyncAccount.load(dir))
    var busy by mutableStateOf(false)
    var error by mutableStateOf<String?>(null)

    private fun now() = System.currentTimeMillis()

    fun login(name: String, password: String, uri: Uri) {
        if (busy) return
        busy = true
        error = null
        worker.execute {
            try {
                val bytes = engine.read(uri) ?: throw SyncError("файл не открылся — проверьте, что OneDrive его скачал")
                val salt = SyncCrypto.readSalt(bytes) ?: throw SyncError("это не файл синхронизации Rust Browser")
                val key = SyncCrypto.deriveKey(password, salt)
                SyncCrypto.decrypt(bytes, key) // неверный пароль — исключение
                val (sealed, iv) = Keystore.seal(key)
                val acc = SyncAccount(name.trim(), uri, salt, sealed, iv)
                main.post {
                    account = acc
                    save()
                    busy = false
                    syncNow(manual = true)
                }
            } catch (e: Exception) {
                main.post { busy = false; error = e.message ?: "не удалось войти" }
            }
        }
    }

    fun logout() {
        account = null
        File(dir, "account.json").delete()
        error = null
    }

    /** Раз в 5 минут при возвращении в приложение и при уходе из него. */
    fun syncIfStale(maxAgeMs: Long) {
        val acc = account ?: return
        if (now() - acc.synced * 1000 > maxAgeMs) syncNow(manual = false)
    }

    fun syncNow(manual: Boolean) {
        val acc = account ?: return
        if (busy) return
        busy = true
        // снимок своих данных — здесь, на главном потоке
        val local = engine.localSections(store.bookmarks.toList(), store.history.toList(), store.historyCleared)
        val defaultsOnly = store.bookmarksAreDefaults()
        val hashes = acc.hashes.toMap()
        worker.execute {
            try {
                val key = acc.key()
                val bytes = engine.read(acc.uri) ?: throw SyncError("файл в облаке недоступен — откройте OneDrive и повторите")
                val bundle = SyncCrypto.decrypt(bytes, key)
                val sections = bundle.optJSONObject("sections") ?: JSONObject()
                val results = LinkedHashMap<String, Any>()
                for ((name, value) in local) {
                    val remote = sections.opt(name)
                    var choice = engine.choose(value, remote, hashes[name])
                    // первые плитки телефона не разносим по компьютерам
                    if (name == "bookmarks" && hashes[name] == null && defaultsOnly && remote != null) choice = "remote"
                    results[name] = when (choice) {
                        "local" -> value
                        "remote" -> remote!!
                        else -> engine.merge(name, value, remote!!)
                    }
                }
                // в облако — все разделы компьютера как были, поверх — наши слитые
                val out = JSONObject(sections.toString())
                results.forEach { (k, v) -> out.put(k, v) }
                val needsWrite = results.any { (k, v) -> !Json.same(v, sections.opt(k)) }
                var writeError: String? = null
                if (needsWrite) {
                    val fresh = JSONObject().put("sections", out).put("device", engine.device()).put("time", now() / 1000)
                    try {
                        engine.write(acc.uri, SyncCrypto.encrypt(fresh, key, acc.salt))
                    } catch (e: Exception) {
                        writeError = "с компьютера получено, но изменения телефона не записались: ${e.message}"
                    }
                }
                val newHashes = results.mapValues { Json.fingerprint(it.value) }.toMutableMap()
                main.post {
                    val changed = apply(results, local)
                    // не записалось — старые отпечатки, в следующий раз попробуем снова
                    if (writeError == null) acc.hashes = newHashes
                    acc.synced = now() / 1000
                    save()
                    busy = false
                    error = writeError
                    if (manual && writeError == null) {
                        val text = if (changed > 0) "Синхронизировано: обновлено разделов — $changed" else "Всё синхронизировано"
                        android.widget.Toast.makeText(context, text, android.widget.Toast.LENGTH_SHORT).show()
                    }
                }
            } catch (e: Exception) {
                main.post { busy = false; error = e.message ?: "синхронизация не удалась" }
            }
        }
    }

    /** Применяет итог к закладкам и истории; возвращает, сколько разделов поменялось. */
    private fun apply(results: Map<String, Any>, local: Map<String, Any>): Int {
        var changed = 0
        results["bookmarks"]?.let { v ->
            if (!Json.same(v, local["bookmarks"])) {
                val a = v as JSONArray
                store.replaceBookmarks((0 until a.length()).mapNotNull { a.optJSONObject(it) }.map { Bookmark(it.optString("url"), it.optString("title")) })
                changed++
            }
        }
        results["history"]?.let { v ->
            if (!Json.same(v, local["history"])) {
                val h = v as JSONObject
                val a = h.optJSONArray("visits") ?: JSONArray()
                val visits = (0 until a.length()).mapNotNull { a.optJSONObject(it) }
                    .map { Visit(it.optString("url"), it.optString("title"), it.optLong("time") * 1000) }
                    .sortedByDescending { it.time }
                store.replaceHistory(visits, h.optLong("cleared"))
                changed++
            }
        }
        return changed
    }

    private fun save() {
        val acc = account ?: return
        val tmp = File(dir, "account.json.tmp")
        tmp.writeText(acc.toJson().toString())
        tmp.renameTo(File(dir, "account.json"))
    }
}
