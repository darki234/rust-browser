package dev.rustbrowser

import android.content.Context
import android.net.Uri
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.security.KeyStore
import java.security.MessageDigest
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.Mac
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec

/**
 * Синхронизация с настольным Rust Browser через тот же зашифрованный файл в OneDrive
 * (формат rust-browser-sync/1 из src/sync.rs): PBKDF2-SHA256 600 000 повторов → AES-256-GCM.
 *
 * Телефон сливает закладки и историю по тем же правилам, что и компьютер. Остальные
 * разделы (пароли, расширения, автозаполнение, параметры, заметки) он не трогает
 * и записывает обратно как есть — ничего не теряется.
 */
object SyncCrypto {
    const val FORMAT = "rust-browser-sync/1"
    const val ITERATIONS = 600_000

    /** PBKDF2-HMAC-SHA256 над байтами UTF-8 пароля — ровно как pbkdf2_hmac в Rust. */
    fun deriveKey(password: String, salt: ByteArray, iterations: Int = ITERATIONS): ByteArray {
        val mac = Mac.getInstance("HmacSHA256")
        mac.init(SecretKeySpec(password.toByteArray(Charsets.UTF_8), "HmacSHA256"))
        // один блок: длина ключа 32 байта = длина SHA-256
        mac.update(salt)
        mac.update(byteArrayOf(0, 0, 0, 1))
        var u = mac.doFinal()
        val out = u.copyOf()
        for (i in 1 until iterations) {
            u = mac.doFinal(u)
            for (j in out.indices) out[j] = (out[j].toInt() xor u[j].toInt()).toByte()
        }
        return out
    }

    fun decrypt(bytes: ByteArray, key: ByteArray): JSONObject {
        val env = runCatching { JSONObject(String(bytes, Charsets.UTF_8)) }.getOrNull() ?: throw SyncError("файл синхронизации повреждён")
        if (env.optString("format") != FORMAT) throw SyncError("файл от более новой версии браузера")
        val nonce = b64(env.optString("nonce"))
        val sealed = b64(env.optString("data"))
        if (nonce.size != 12) throw SyncError("файл повреждён")
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, SecretKeySpec(key, "AES"), GCMParameterSpec(128, nonce))
        val plain = runCatching { cipher.doFinal(sealed) }.getOrNull() ?: throw SyncError("неверный пароль")
        return JSONObject(String(plain, Charsets.UTF_8))
    }

    fun encrypt(bundle: JSONObject, key: ByteArray, salt: ByteArray): ByteArray {
        val nonce = ByteArray(12).also { SecureRandom().nextBytes(it) }
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, SecretKeySpec(key, "AES"), GCMParameterSpec(128, nonce))
        val sealed = cipher.doFinal(bundle.toString().toByteArray(Charsets.UTF_8))
        val env = JSONObject()
            .put("format", FORMAT).put("kdf", "pbkdf2-sha256").put("iterations", ITERATIONS)
            .put("salt", b64(salt)).put("nonce", b64(nonce)).put("data", b64(sealed))
        return env.toString().toByteArray(Charsets.UTF_8)
    }

    fun readSalt(bytes: ByteArray): ByteArray? = runCatching { b64(JSONObject(String(bytes, Charsets.UTF_8)).getString("salt")) }.getOrNull()

    /** Имя файла аккаунта, как его даёт компьютер: RustBrowser-<8 байт SHA-256 имени>.rbsync */
    fun fileName(account: String): String {
        val digest = MessageDigest.getInstance("SHA-256").digest(account.trim().lowercase().toByteArray(Charsets.UTF_8))
        return "RustBrowser-" + digest.take(8).joinToString("") { "%02x".format(it) } + ".rbsync"
    }

    fun b64(bytes: ByteArray): String = Base64.encodeToString(bytes, Base64.NO_WRAP)
    fun b64(text: String): ByteArray = runCatching { Base64.decode(text, Base64.DEFAULT) }.getOrDefault(ByteArray(0))
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

// ---------- аккаунт на телефоне ----------

/** Аккаунт: файл в облаке (адрес от системы), соль, ключ под защитой Android Keystore. */
class SyncAccount(
    val name: String,
    val uri: Uri,
    val salt: ByteArray,
    val sealedKey: ByteArray,
    val keyIv: ByteArray,
    var hashes: MutableMap<String, String> = mutableMapOf(),
    var synced: Long = 0,
) {
    fun toJson(): JSONObject = JSONObject()
        .put("name", name).put("uri", uri.toString())
        .put("salt", SyncCrypto.b64(salt)).put("key", SyncCrypto.b64(sealedKey)).put("iv", SyncCrypto.b64(keyIv))
        .put("hashes", JSONObject(hashes as Map<*, *>)).put("synced", synced)

    fun key(): ByteArray = Keystore.open(sealedKey, keyIv)

    companion object {
        fun load(dir: File): SyncAccount? = runCatching {
            val j = JSONObject(File(dir, "account.json").readText())
            val hashes = j.optJSONObject("hashes")?.let { h -> h.keys().asSequence().associateWith { h.getString(it) }.toMutableMap() } ?: mutableMapOf()
            SyncAccount(j.getString("name"), Uri.parse(j.getString("uri")), SyncCrypto.b64(j.getString("salt")),
                SyncCrypto.b64(j.getString("key")), SyncCrypto.b64(j.getString("iv")), hashes, j.optLong("synced"))
        }.getOrNull()
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

    fun read(uri: Uri): ByteArray? = runCatching { context.contentResolver.openInputStream(uri)?.use { it.readBytes() } }.getOrNull()

    fun write(uri: Uri, bytes: ByteArray) {
        val out = context.contentResolver.openOutputStream(uri, "wt") ?: throw SyncError("облако не дало записать файл")
        out.use { it.write(bytes) }
    }

    fun device(): String = "${Build.MANUFACTURER} ${Build.MODEL}".trim()
}
