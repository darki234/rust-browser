package dev.rustbrowser

import android.content.Context
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.HttpURLConnection
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.Socket
import java.net.URL
import java.security.KeyStore
import java.security.MessageDigest
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec

/**
 * Синхронизация с настольным Rust Browser (src/sync.rs, формат rust-browser-sync/2).
 *
 * Телефон входит в «связку» компьютера по QR-коду: в коде случайный ключ AES-256,
 * номер связки, адреса компьютера в домашней сети и (если подключён) диск WebDAV.
 * Дома телефон обменивается с компьютером напрямую, вне дома — через файл на диске.
 * Всё зашифровано ключом связки, свои «точки отсчёта» слияния для компьютера и диска.
 */
object SyncCrypto {
    const val FORMAT = "rust-browser-sync/2"

    fun decrypt(bytes: ByteArray, key: ByteArray): JSONObject {
        val env = runCatching { JSONObject(String(bytes, Charsets.UTF_8)) }.getOrNull() ?: throw SyncError("данные синхронизации повреждены")
        if (env.optString("format") != FORMAT) throw SyncError("данные от другой версии браузера — обновите его")
        val nonce = b64(env.optString("nonce"))
        val sealed = b64(env.optString("data"))
        if (nonce.size != 12) throw SyncError("данные повреждены")
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, SecretKeySpec(key, "AES"), GCMParameterSpec(128, nonce))
        val plain = runCatching { cipher.doFinal(sealed) }.getOrNull() ?: throw SyncError("чужой ключ — подключите телефон заново по QR-коду")
        return JSONObject(String(plain, Charsets.UTF_8))
    }

    fun encrypt(bundle: JSONObject, key: ByteArray, group: String): ByteArray {
        val nonce = random(12)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, SecretKeySpec(key, "AES"), GCMParameterSpec(128, nonce))
        val sealed = cipher.doFinal(bundle.toString().toByteArray(Charsets.UTF_8))
        val env = JSONObject().put("format", FORMAT).put("group", group).put("nonce", b64(nonce)).put("data", b64(sealed))
        return env.toString().toByteArray(Charsets.UTF_8)
    }

    fun b64(bytes: ByteArray): String = Base64.encodeToString(bytes, Base64.NO_WRAP)
    fun b64(text: String): ByteArray = runCatching { Base64.decode(text, Base64.DEFAULT) }.getOrDefault(ByteArray(0))
    fun hex(bytes: ByteArray): String = bytes.joinToString("") { "%02x".format(it) }
    fun random(n: Int): ByteArray = ByteArray(n).also { SecureRandom().nextBytes(it) }
}

/** Код связки из QR (RBSYNC1.<base64url JSON>), как его даёт компьютер. */
class PairCode(
    val group: String,
    val key: ByteArray,
    val pcName: String,
    val hosts: List<String>,
    val port: Int,
    val dav: Triple<String, String, String>?, // адрес, логин, пароль
) {
    companion object {
        fun parse(text: String): PairCode? = runCatching {
            val trimmed = text.trim()
            if (!trimmed.startsWith("RBSYNC1.")) return null
            val body = trimmed.removePrefix("RBSYNC1.")
            val json = JSONObject(String(Base64.decode(body, Base64.URL_SAFE or Base64.NO_PADDING or Base64.NO_WRAP), Charsets.UTF_8))
            val key = SyncCrypto.b64(json.getString("k"))
            val group = json.optString("g")
            // код мог прислать кто угодно: номер связки — короткий hex, диск — https
            // (http — только в домашней сети), без управляющих символов
            if (key.size != 32 || group.isEmpty() || group.length > 32 || !group.all { it.isDigit() || it.lowercaseChar() in 'a'..'f' }) return null
            val a = json.optJSONArray("a") ?: JSONArray()
            val w = json.optJSONObject("w")
            val dav = w?.let { Triple(it.optString("u").trim(), it.optString("l"), it.optString("p")) }
            if (dav != null && (!davUrlOk(dav.first) || dav.second.any { it.isISOControl() } || dav.third.any { it.isISOControl() })) return null
            val hosts = (0 until a.length()).map { a.getString(it) }.filter { h -> h.length <= 45 && h.all { it.isLetterOrDigit() || it == '.' || it == ':' } }.take(8)
            PairCode(
                group, key, json.optString("n").filter { !it.isISOControl() }.take(64),
                hosts, json.optInt("p", Lan.PORT).takeIf { it in 1..65535 } ?: Lan.PORT,
                dav,
            )
        }.getOrNull()

        /** https — всегда; http — только адрес в домашней сети (иначе пароль идёт открытым текстом). */
        fun davUrlOk(url: String): Boolean {
            if (url.any { it.isISOControl() || it.isWhitespace() } || url.length > 2000) return false
            if (url.startsWith("https://")) return url.length > 8
            val host = url.removePrefix("http://").takeIf { it != url }?.substringBefore('/')?.substringBefore(':') ?: return false
            if (host == "localhost") return true
            val p = host.split('.').mapNotNull { it.toIntOrNull() }
            if (p.size != 4) return false
            return p[0] == 10 || p[0] == 127 || (p[0] == 192 && p[1] == 168) || (p[0] == 172 && p[1] in 16..31)
        }
    }
}

class SyncError(message: String) : Exception(message)

// ---------- JSON: каноническая запись для отпечатков, сравнение, слияние ----------

object Json {
    /** Как serde_json: ключи по алфавиту, без пробелов, «/» не экранируется. */
    fun canonical(v: Any?): String = StringBuilder().also { write(it, v) }.toString()

    private fun write(sb: StringBuilder, v: Any?) {
        when (v) {
            null, JSONObject.NULL -> sb.append("null")
            is JSONObject -> {
                sb.append('{')
                v.keys().asSequence().sorted().forEachIndexed { i, k ->
                    if (i > 0) sb.append(',')
                    string(sb, k); sb.append(':'); write(sb, v.opt(k))
                }
                sb.append('}')
            }
            is JSONArray -> {
                sb.append('[')
                for (i in 0 until v.length()) { if (i > 0) sb.append(','); write(sb, v.opt(i)) }
                sb.append(']')
            }
            is String -> string(sb, v)
            is Boolean -> sb.append(v)
            is Int, is Long -> sb.append(v)
            is Number -> {
                val d = v.toDouble()
                if (d == Math.floor(d) && !d.isInfinite() && kotlin.math.abs(d) < 1e15) sb.append(d.toLong().toString() + ".0") else sb.append(d)
            }
            else -> string(sb, v.toString())
        }
    }

    private fun string(sb: StringBuilder, s: String) {
        sb.append('"')
        for (c in s) when (c) {
            '"' -> sb.append("\\\""); '\\' -> sb.append("\\\\")
            '\n' -> sb.append("\\n"); '\r' -> sb.append("\\r"); '\t' -> sb.append("\\t")
            '\b' -> sb.append("\\b"); '\u000C' -> sb.append("\\f")
            else -> if (c < ' ') sb.append("\\u%04x".format(c.code)) else sb.append(c)
        }
        sb.append('"')
    }

    fun fingerprint(v: Any?): String =
        MessageDigest.getInstance("SHA-256").digest(canonical(v).toByteArray(Charsets.UTF_8)).take(12).joinToString("") { "%02x".format(it) }

    fun same(a: Any?, b: Any?) = canonical(a) == canonical(b)

    /** Списки по ключу: сначала облачные, потом свои, которых там нет (sync::union_by). */
    fun unionBy(remote: Any?, local: Any?, key: (JSONObject) -> String): JSONArray {
        val out = JSONArray()
        val seen = HashSet<String>()
        for (list in listOf(remote, local)) {
            val a = list as? JSONArray ?: continue
            for (i in 0 until a.length()) {
                val item = a.optJSONObject(i) ?: continue
                if (seen.add(key(item))) out.put(item)
            }
        }
        return out
    }
}

// ---------- связка на телефоне ----------

/** Диск WebDAV: адрес, логин, пароль под Keystore. */
class Dav(val url: String, val user: String, val sealedPass: ByteArray, val passIv: ByteArray) {
    fun pass(): String = String(Keystore.open(sealedPass, passIv), Charsets.UTF_8)
    fun file(group: String) = url.trim().trimEnd('/') + "/rust-browser-$group.rbsync"
    fun toJson(): JSONObject = JSONObject().put("url", url).put("user", user).put("pass", SyncCrypto.b64(sealedPass)).put("iv", SyncCrypto.b64(passIv))

    companion object {
        fun from(j: JSONObject?): Dav? = j?.let { Dav(it.getString("url"), it.getString("user"), SyncCrypto.b64(it.getString("pass")), SyncCrypto.b64(it.getString("iv"))) }
    }
}

/** Связка: номер, ключ под Keystore, компьютер в домашней сети, диск, точки отсчёта слияния. */
class SyncAccount(
    val group: String,
    val sealedKey: ByteArray,
    val keyIv: ByteArray,
    val device: String,
    val pcName: String,
    var hosts: List<String>,
    var port: Int,
    var dav: Dav?,
    var bases: MutableMap<String, Map<String, String>> = mutableMapOf(),
    var synced: Long = 0,
    /** Как прошла последняя синхронизация: "lan" — по Wi‑Fi, "dav" — через диск. */
    var via: String = "",
) {
    fun toJson(): JSONObject {
        val b = JSONObject()
        bases.forEach { (peer, map) -> b.put(peer, JSONObject(map as Map<*, *>)) }
        return JSONObject()
            .put("group", group).put("key", SyncCrypto.b64(sealedKey)).put("iv", SyncCrypto.b64(keyIv))
            .put("device", device).put("pc", pcName).put("hosts", JSONArray(hosts)).put("port", port)
            .put("dav", dav?.toJson() ?: JSONObject.NULL).put("bases", b).put("synced", synced).put("via", via)
    }

    fun key(): ByteArray = Keystore.open(sealedKey, keyIv)

    companion object {
        fun load(dir: File): SyncAccount? = runCatching {
            val j = JSONObject(File(dir, "account.json").readText())
            if (j.optString("group").isEmpty()) return null // старый вход через OneDrive больше не используется
            val bases = mutableMapOf<String, Map<String, String>>()
            j.optJSONObject("bases")?.let { b ->
                b.keys().forEach { peer -> val m = b.getJSONObject(peer); bases[peer] = m.keys().asSequence().associateWith { m.getString(it) } }
            }
            val hosts = j.optJSONArray("hosts")?.let { a -> (0 until a.length()).map { a.getString(it) } } ?: emptyList()
            SyncAccount(j.getString("group"), SyncCrypto.b64(j.getString("key")), SyncCrypto.b64(j.getString("iv")), j.getString("device"),
                j.optString("pc"), hosts, j.optInt("port", Lan.PORT), Dav.from(j.optJSONObject("dav")), bases, j.optLong("synced"), j.optString("via"))
        }.getOrNull()
    }
}

// ---------- доставка: домашняя сеть и диск ----------

/** Компьютер в домашней сети: простой HTTP поверх сокета (данные и так зашифрованы). */
object Lan {
    const val PORT = 47801

    private fun request(host: String, port: Int, head: String, body: ByteArray, timeout: Int): Pair<Int, ByteArray>? = runCatching {
        Socket().use { s ->
            s.connect(InetSocketAddress(host, port), timeout)
            s.soTimeout = 20_000
            val out = s.getOutputStream()
            out.write((head + "Content-Length: ${body.size}\r\nConnection: close\r\n\r\n").toByteArray(Charsets.UTF_8))
            out.write(body)
            out.flush()
            val raw = s.getInputStream().readBytes()
            var split = -1
            for (i in 0 until raw.size - 3) {
                if (raw[i] == 13.toByte() && raw[i + 1] == 10.toByte() && raw[i + 2] == 13.toByte() && raw[i + 3] == 10.toByte()) { split = i; break }
            }
            if (split < 0) return null
            val status = String(raw, 0, split, Charsets.UTF_8).split(" ").getOrNull(1)?.toIntOrNull() ?: 0
            status to raw.copyOfRange(split + 4, raw.size)
        }
    }.getOrNull()

    /** Номер версии данных на компьютере — дёшево узнать, есть ли новое. */
    fun rev(host: String, port: Int, group: String): Long? {
        val (status, body) = request(host, port, "GET /rb-rev HTTP/1.1\r\nHost: $host\r\nX-RB-Group: $group\r\n", ByteArray(0), 1500) ?: return null
        return if (status == 200) String(body).trim().toLongOrNull() else null
    }

    fun exchange(host: String, port: Int, group: String, packet: ByteArray): ByteArray? {
        val (status, body) = request(host, port, "POST /rb-sync HTTP/1.1\r\nHost: $host\r\nX-RB-Group: $group\r\n", packet, 2500) ?: return null
        return if (status == 200) body else null
    }

    /** Компьютер сменил адрес — кричим на всю сеть, он отвечает своим портом. */
    fun discover(group: String): Pair<String, Int>? = runCatching {
        DatagramSocket().use { s ->
            s.broadcast = true
            s.soTimeout = 1200
            val msg = "rb-find $group".toByteArray()
            s.send(DatagramPacket(msg, msg.size, InetAddress.getByName("255.255.255.255"), PORT))
            val buf = ByteArray(128)
            val packet = DatagramPacket(buf, buf.size)
            s.receive(packet)
            val text = String(buf, 0, packet.length)
            val port = text.removePrefix("rb-here ").trim().toIntOrNull() ?: return null
            packet.address.hostAddress!! to port
        }
    }.getOrNull()

    /** Эмулятор Android видит компьютер, на котором запущен, по адресу 10.0.2.2. */
    fun emulator(): Boolean = Build.FINGERPRINT.contains("generic") || Build.MODEL.contains("sdk_gphone") || Build.HARDWARE.contains("ranchu")
}

/** Диск WebDAV: файл связки скачать и записать. */
object WebDav {
    private fun open(url: String, user: String, pass: String, method: String): HttpURLConnection {
        val c = URL(url).openConnection() as HttpURLConnection
        c.requestMethod = method
        c.connectTimeout = 10_000
        c.readTimeout = 20_000
        val auth = Base64.encodeToString("$user:$pass".toByteArray(Charsets.UTF_8), Base64.NO_WRAP)
        c.setRequestProperty("Authorization", "Basic $auth")
        return c
    }

    private fun fail(code: Int): Nothing = throw SyncError(when (code) {
        401, 403 -> "диск не пустил: проверьте логин и пароль приложения"
        404, 409 -> "на диске нет такой папки"
        507 -> "на диске закончилось место"
        else -> "диск ответил ошибкой $code"
    })

    /** null — файла ещё нет. */
    fun get(url: String, user: String, pass: String): ByteArray? {
        val c = open(url, user, pass, "GET")
        try {
            val code = c.responseCode
            if (code == 404) return null
            if (code != 200) fail(code)
            return c.inputStream.use { it.readBytes() }
        } finally { c.disconnect() }
    }

    fun put(url: String, user: String, pass: String, bytes: ByteArray) {
        val c = open(url, user, pass, "PUT")
        try {
            c.doOutput = true
            c.setFixedLengthStreamingMode(bytes.size)
            c.outputStream.use { it.write(bytes) }
            val code = c.responseCode
            if (code !in 200..299) fail(code)
        } finally { c.disconnect() }
    }
}

/** Ключ синхронизации шифруется ключом из Android Keystore: из файлов приложения его не достать. */
object Keystore {
    private const val ALIAS = "rustbrowser-sync"

    private fun secret(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build())
        return gen.generateKey()
    }

    fun seal(plain: ByteArray): Pair<ByteArray, ByteArray> {
        val c = Cipher.getInstance("AES/GCM/NoPadding")
        c.init(Cipher.ENCRYPT_MODE, secret())
        return c.doFinal(plain) to c.iv
    }

    fun open(sealed: ByteArray, iv: ByteArray): ByteArray {
        val c = Cipher.getInstance("AES/GCM/NoPadding")
        c.init(Cipher.DECRYPT_MODE, secret(), GCMParameterSpec(128, iv))
        return c.doFinal(sealed)
    }
}

/** Разделы этого телефона и их применение. Работает в фоновом потоке, кроме [apply]. */
class SyncEngine(private val context: Context, private val store: Store) {

    /** То, что телефон отдаёт в облако: закладки, история (время — в секундах, как у компьютера),
     *  пароли и отправленные вкладки. */
    fun localSections(bookmarks: List<Bookmark>, history: List<Visit>, cleared: Long, passwords: JSONObject, sent: JSONArray): Map<String, Any> {
        val b = JSONArray()
        bookmarks.forEach { b.put(JSONObject().put("title", it.title).put("url", it.url)) }
        val v = JSONArray()
        history.forEach { v.put(JSONObject().put("url", it.url).put("title", it.title).put("time", it.time / 1000)) }
        return mapOf(
            "bookmarks" to b,
            "history" to JSONObject().put("cleared", cleared).put("visits", v),
            "passwords" to passwords,
            "sent_tabs" to fresh(sent),
        )
    }

    /** Отправленные вкладки живут неделю (как у компьютера). */
    fun fresh(list: JSONArray): JSONArray {
        val weekAgo = System.currentTimeMillis() / 1000 - 7 * 24 * 3600
        val out = JSONArray()
        for (i in 0 until list.length()) list.optJSONObject(i)?.takeIf { it.optLong("time") > weekAgo }?.let { if (out.length() < 200) out.put(it) }
        return out
    }

    fun merge(name: String, local: Any, remote: Any): Any = when (name) {
        "bookmarks" -> Json.unionBy(remote, local) { it.optString("url") }
        "history" -> {
            val r = remote as? JSONObject ?: JSONObject()
            val l = local as JSONObject
            val cleared = maxOf(r.optLong("cleared"), l.optLong("cleared"))
            val visits = Json.unionBy(r.optJSONArray("visits"), l.optJSONArray("visits")) { it.optString("url") + "|" + it.optLong("time") }
            val kept = JSONArray()
            for (i in 0 until visits.length()) visits.getJSONObject(i).takeIf { it.optLong("time") > cleared }?.let { kept.put(it) }
            JSONObject().put("cleared", cleared).put("visits", kept)
        }
        "passwords" -> {
            val r = remote as? JSONObject ?: JSONObject()
            val l = local as JSONObject
            val never = JSONArray()
            val seen = HashSet<String>()
            for (list in listOf(r.optJSONArray("never"), l.optJSONArray("never"))) {
                list ?: continue
                for (i in 0 until list.length()) list.optString(i).takeIf { it.isNotEmpty() && seen.add(it) }?.let { never.put(it) }
            }
            JSONObject()
                .put("logins", Json.unionBy(r.optJSONArray("logins"), l.optJSONArray("logins")) { it.optString("origin") + "|" + it.optString("username") })
                .put("never", never)
        }
        "sent_tabs" -> fresh(Json.unionBy(remote, local) { it.optString("id") })
        else -> remote
    }

    /** Чья версия раздела побеждает (sync::choose). */
    fun choose(local: Any, remote: Any?, last: String?): String {
        remote ?: return "local"
        val lh = Json.fingerprint(local); val rh = Json.fingerprint(remote)
        return when {
            lh == rh -> "local"
            last == lh -> "remote"
            last == rh -> "local"
            else -> "merge"
        }
    }

    fun device(): String = "${Build.MANUFACTURER} ${Build.MODEL}".trim()
}
