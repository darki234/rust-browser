package dev.rustbrowser

import android.content.Context
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
 * Синхронизация с компьютером: сначала напрямую по Wi‑Fi, потом через диск WebDAV.
 * Сеть и шифр — в фоновом потоке; на главном только снимок данных и применение итога.
 * Пока приложение открыто, телефон раз в несколько секунд спрашивает у компьютера номер
 * версии данных — вкладка, отправленная с компьютера, приходит почти сразу.
 */
class SyncManager(private val context: Context, private val store: Store, private val passwords: Passwords) {
    /** Пришли вкладки с компьютера: (адрес, заголовок). Открывает их браузер. */
    var onReceived: (List<Pair<String, String>>) -> Unit = {}
    private val dir: File = context.filesDir
    private val engine = SyncEngine(context, store)
    private val worker = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())

    var account by mutableStateOf(SyncAccount.load(dir))
    var busy by mutableStateOf(false)
    var error by mutableStateOf<String?>(null)

    private fun now() = System.currentTimeMillis()

    /** Вход в связку по коду из QR. */
    fun pair(code: String): Boolean {
        val pair = PairCode.parse(code) ?: run { error = "Это не код Rust Browser — откройте на компьютере Параметры → Синхронизация"; return false }
        val (sealed, iv) = Keystore.seal(pair.key)
        val dav = pair.dav?.let { (url, user, pass) ->
            val (sp, si) = Keystore.seal(pass.toByteArray(Charsets.UTF_8))
            Dav(url, user, sp, si)
        }
        account = SyncAccount(pair.group, sealed, iv, SyncCrypto.hex(SyncCrypto.random(6)), pair.pcName, pair.hosts, pair.port, dav)
        error = null
        save()
        syncNow(manual = true)
        return true
    }

    fun logout() {
        account = null
        File(dir, "account.json").delete()
        error = null
    }

    /** При возвращении в приложение и при уходе из него — если давно не было. */
    fun syncIfStale(maxAgeMs: Long) {
        val acc = account ?: return
        if (now() - acc.synced * 1000 > maxAgeMs) syncNow(manual = false)
    }

    // ---------- опрос компьютера ----------

    private var polling = false
    private var lastRev: Long? = null
    private val poll = object : Runnable {
        override fun run() {
            if (!polling) return
            val acc = account
            if (acc != null && !busy) {
                val key = acc.group
                worker.execute {
                    val rev = lanHosts(acc).firstNotNullOfOrNull { host -> Lan.rev(host, acc.port, key) }
                    main.post {
                        if (rev != null && rev != lastRev) {
                            val first = lastRev == null
                            lastRev = rev
                            if (!first) syncNow(manual = false, lanOnly = true)
                        }
                    }
                }
            }
            main.postDelayed(this, 5_000)
        }
    }

    fun startPolling() {
        if (polling) return
        polling = true
        main.postDelayed(poll, 1_500)
    }

    fun stopPolling() {
        polling = false
        main.removeCallbacks(poll)
    }

    /** Где искать компьютер: адреса из кода, а на эмуляторе — сам компьютер. */
    private fun lanHosts(acc: SyncAccount): List<String> = (if (Lan.emulator()) listOf("10.0.2.2") else emptyList()) + acc.hosts

    // ---------- синхронизация ----------

    private fun localNow(): Map<String, Any> =
        engine.localSections(store.bookmarks.toList(), store.history.toList(), store.historyCleared, passwords.toJson(), store.sentTabs)

    /** Wi‑Fi, затем диск. [lanOnly] — когда компьютер сам сказал, что у него новое. */
    fun syncNow(manual: Boolean, lanOnly: Boolean = false) {
        val acc = account ?: return
        if (busy) return
        busy = true
        lanPhase(acc, manual) { lanChanged, lanOk ->
            if (lanOnly || acc.dav == null) {
                finish(acc, manual, lanChanged, if (lanOk) null else if (acc.dav == null) "компьютер не найден в этой сети Wi‑Fi" else null)
            } else {
                davPhase(acc, manual, lanChanged, lanOk)
            }
        }
    }

    /** Слить ответ собеседника [peer] со своими данными; итог — на главном потоке. */
    private fun mergeWith(remote: JSONObject, local: Map<String, Any>, base: Map<String, String>, defaultsOnly: Boolean): Map<String, Any> {
        val results = LinkedHashMap<String, Any>()
        for ((name, value) in local) {
            val r = remote.opt(name)
            var choice = engine.choose(value, r, base[name])
            // первые плитки телефона не разносим по компьютерам
            if (name == "bookmarks" && base[name] == null && defaultsOnly && r != null) choice = "remote"
            results[name] = when (choice) {
                "local" -> value
                "remote" -> r!!
                else -> engine.merge(name, value, r!!)
            }
        }
        return results
    }

    private fun lanPhase(acc: SyncAccount, manual: Boolean, next: (changed: Int, ok: Boolean) -> Unit) {
        val local = localNow()
        val defaultsOnly = store.bookmarksAreDefaults()
        val base = acc.bases["pc"] ?: emptyMap()
        worker.execute {
            try {
                val key = acc.key()
                val sections = JSONObject()
                local.forEach { (k, v) -> sections.put(k, v) }
                val packet = SyncCrypto.encrypt(
                    JSONObject().put("sections", sections).put("device", engine.device()).put("id", acc.device).put("time", now() / 1000),
                    key, acc.group,
                )
                var answer: ByteArray? = null
                var found: Pair<String, Int>? = null
                for (host in lanHosts(acc)) {
                    answer = Lan.exchange(host, acc.port, acc.group, packet)
                    if (answer != null) { found = host to acc.port; break }
                }
                if (answer == null) {
                    // компьютер мог сменить адрес — ищем его по сети
                    Lan.discover(acc.group)?.let { (host, port) ->
                        answer = Lan.exchange(host, port, acc.group, packet)
                        if (answer != null) found = host to port
                    }
                }
                val bytes = answer
                if (bytes == null) {
                    main.post { next(0, false) }
                    return@execute
                }
                val remote = SyncCrypto.decrypt(bytes, key).optJSONObject("sections") ?: JSONObject()
                val results = mergeWith(remote, local, base, defaultsOnly)
                val hashes = results.mapValues { Json.fingerprint(it.value) }
                main.post {
                    val changed = apply(results, local)
                    acc.bases["pc"] = hashes
                    found?.let { (host, port) ->
                        if (!Lan.emulator() || host != "10.0.2.2") acc.hosts = (listOf(host) + acc.hosts).distinct().take(4)
                        acc.port = port
                    }
                    acc.via = "lan"
                    next(changed, true)
                }
            } catch (e: Exception) {
                main.post { busy = false; error = e.message ?: "синхронизация не удалась" }
            }
        }
    }

    private fun davPhase(acc: SyncAccount, manual: Boolean, lanChanged: Int, lanOk: Boolean) {
        val dav = acc.dav ?: return finish(acc, manual, lanChanged, null)
        val local = localNow()
        val defaultsOnly = store.bookmarksAreDefaults()
        val base = acc.bases["webdav"] ?: emptyMap()
        worker.execute {
            try {
                val key = acc.key()
                val file = dav.file(acc.group)
                val pass = dav.pass()
                val bytes = WebDav.get(file, dav.user, pass)
                val sections = bytes?.let { SyncCrypto.decrypt(it, key).optJSONObject("sections") } ?: JSONObject()
                val results = mergeWith(sections, local, base, defaultsOnly)
                // на диск — все разделы компьютера как были, поверх — наши слитые
                val out = JSONObject(sections.toString())
                results.forEach { (k, v) -> out.put(k, v) }
                val needsWrite = bytes == null || results.any { (k, v) -> !Json.same(v, sections.opt(k)) }
                if (needsWrite) {
                    val fresh = JSONObject().put("sections", out).put("device", engine.device()).put("id", acc.device).put("time", now() / 1000)
                    WebDav.put(file, dav.user, pass, SyncCrypto.encrypt(fresh, key, acc.group))
                }
                val hashes = results.mapValues { Json.fingerprint(it.value) }
                main.post {
                    val changed = apply(results, local)
                    acc.bases["webdav"] = hashes
                    if (!lanOk) acc.via = "dav"
                    finish(acc, manual, lanChanged + changed, null)
                }
            } catch (e: Exception) {
                main.post { finish(acc, manual, lanChanged, if (lanOk) null else e.message ?: "диск недоступен") }
            }
        }
    }

    private fun finish(acc: SyncAccount, manual: Boolean, changed: Int, problem: String?) {
        // пока шёл обмен, синхронизацию выключили — ничего не возвращаем
        if (account !== acc) { busy = false; return }
        if (problem == null) acc.synced = now() / 1000
        save()
        busy = false
        error = problem
        // снимок для Compose: тот же объект, но с новыми полями
        account = null
        account = acc
        if (manual && problem == null) {
            val text = if (changed > 0) "Синхронизировано: обновлено разделов — $changed" else "Всё синхронизировано"
            android.widget.Toast.makeText(context, text, android.widget.Toast.LENGTH_SHORT).show()
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
        results["passwords"]?.let { v ->
            if (!Json.same(v, local["passwords"])) {
                passwords.replace(v as JSONObject)
                passwords.save()
                changed++
            }
        }
        results["sent_tabs"]?.let { v ->
            val list = v as JSONArray
            if (!Json.same(list, local["sent_tabs"])) store.replaceSentTabs(list)
            openReceived()
        }
        return changed
    }

    /** Вкладки, отправленные на телефон и ещё не открытые. */
    private fun openReceived() {
        val incoming = ArrayList<Pair<String, String>>()
        val ids = ArrayList<String>()
        val list = store.sentTabs
        for (i in 0 until list.length()) {
            val t = list.optJSONObject(i) ?: continue
            val id = t.optString("id")
            val url = t.optString("url")
            if (t.optString("to") != "phone" || id.isEmpty() || id in store.sentHandled || !url.startsWith("http")) continue
            incoming += url to t.optString("title")
            ids += id
        }
        if (ids.isEmpty()) return
        store.markHandled(ids)
        onReceived(incoming)
    }

    /** «На компьютер»: записать и сразу синхронизировать. */
    fun sendToPc(url: String, title: String): Boolean {
        if (account == null) return false
        store.addSentTab(url, title, engine.device())
        syncNow(manual = false)
        return true
    }

    private fun save() {
        val acc = account ?: return
        val tmp = File(dir, "account.json.tmp")
        tmp.writeText(acc.toJson().toString())
        tmp.renameTo(File(dir, "account.json"))
    }
}
