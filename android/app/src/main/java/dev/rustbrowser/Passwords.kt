package dev.rustbrowser

import android.net.Uri
import androidx.compose.runtime.mutableStateListOf
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

/** Сохранённый вход: сайт (https://host), логин, пароль. */
data class Login(val origin: String, val username: String, val password: String, val created: Long)

/**
 * Пароли телефона. Файл зашифрован ключом из Android Keystore — из памяти телефона
 * его не прочитать даже с root без самого телефона. Формат разделов — как у компьютера
 * (src/passwords.rs), поэтому пароли ходят между ними через синхронизацию.
 */
class Passwords(private val dir: File) {
    val logins = mutableStateListOf<Login>()
    /** Сайты, где пользователь сказал «никогда не сохранять». */
    val never = mutableStateListOf<String>()

    init { load() }

    private fun load() {
        val file = File(dir, "passwords.bin")
        if (!file.exists()) return
        runCatching {
            val bytes = file.readBytes()
            val iv = bytes.copyOfRange(1, 1 + bytes[0])
            val json = JSONObject(String(Keystore.open(bytes.copyOfRange(1 + bytes[0], bytes.size), iv), Charsets.UTF_8))
            replace(json)
        }
    }

    fun save() {
        val (sealed, iv) = Keystore.seal(toJson().toString().toByteArray(Charsets.UTF_8))
        val tmp = File(dir, "passwords.bin.tmp")
        tmp.writeBytes(byteArrayOf(iv.size.toByte()) + iv + sealed)
        tmp.renameTo(File(dir, "passwords.bin"))
    }

    /** Раздел синхронизации: { logins: [...], never: [...] }, как у компьютера. */
    fun toJson(): JSONObject {
        val a = JSONArray()
        logins.forEach { a.put(JSONObject().put("origin", it.origin).put("username", it.username).put("password", it.password).put("created", it.created)) }
        val n = JSONArray()
        never.forEach { n.put(it) }
        return JSONObject().put("logins", a).put("never", n)
    }

    fun replace(json: JSONObject) {
        logins.clear(); never.clear()
        val a = json.optJSONArray("logins") ?: JSONArray()
        for (i in 0 until a.length()) a.optJSONObject(i)?.let {
            val origin = it.optString("origin"); val password = it.optString("password")
            if (origin.isNotEmpty() && password.isNotEmpty()) logins += Login(origin, it.optString("username"), password, it.optLong("created"))
        }
        val n = json.optJSONArray("never") ?: JSONArray()
        for (i in 0 until n.length()) n.optString(i).takeIf { it.isNotEmpty() }?.let { never += it }
    }

    fun forOrigin(origin: String) = logins.filter { it.origin == origin }

    /** Новый вход или новый пароль к старому; true — что-то поменялось. */
    fun upsert(origin: String, username: String, password: String): Boolean {
        val i = logins.indexOfFirst { it.origin == origin && it.username == username }
        if (i >= 0) {
            if (logins[i].password == password) return false
            logins[i] = logins[i].copy(password = password)
        } else {
            logins += Login(origin, username, password, System.currentTimeMillis() / 1000)
        }
        save()
        return true
    }

    fun remove(login: Login) { logins.remove(login); save() }

    fun neverFor(origin: String) { if (origin !in never) { never += origin; save() } }

    companion object {
        /** Как passwords::origin_of у компьютера: https://host[:port], для localhost — и http. */
        fun originOf(url: String): String? {
            val uri = Uri.parse(url)
            val host = uri.host?.lowercase() ?: return null
            val port = if (uri.port > 0 && uri.port != 443) ":${uri.port}" else ""
            val local = host == "localhost" || host == "127.0.0.1" || host.endsWith(".localhost")
            return when (uri.scheme) {
                "https" -> "https://$host$port"
                "http" -> if (local) "http://$host${if (uri.port > 0) ":${uri.port}" else ""}" else null
                else -> null
            }
        }
    }
}

/**
 * Скрипт страницы: замечает формы входа и отправку пароля, умеет подставить логин.
 * Сообщения идут через WebMessageListener: телефон знает настоящий адрес отправителя
 * и что это главный кадр — подделать сайт не может.
 */
const val PASSWORD_SCRIPT = """(() => {
  if (window.__rbPw) return; window.__rbPw = 1;
  const post = (o) => { try { rbPw.postMessage(JSON.stringify(o)); } catch (e) {} };
  const visible = (el) => !!(el && (el.offsetWidth || el.offsetHeight || el.getClientRects().length));
  let told = false;
  const scan = () => { if (told) return; const pw = [...document.querySelectorAll('input[type=password]')].find(visible); if (pw) { told = true; post({ cmd: 'form' }); } };
  const userFor = (root, pw) => {
    const inputs = [...root.querySelectorAll('input')].filter(visible);
    const i = inputs.indexOf(pw);
    return inputs.slice(0, i < 0 ? inputs.length : i).reverse().find((x) => /^(text|email|tel|)$/.test(x.type) && x.name !== 'search');
  };
  const grab = (root) => {
    const pw = [...(root || document).querySelectorAll('input[type=password]')].find((p) => p.value);
    if (!pw) return;
    const user = userFor(root || document, pw);
    post({ cmd: 'submit', username: user ? user.value : '', password: pw.value });
  };
  document.addEventListener('submit', (e) => grab(e.target), true);
  document.addEventListener('click', (e) => {
    const b = e.target.closest && e.target.closest('button, input[type=submit], [role=button]');
    if (b) setTimeout(() => grab(b.form || b.closest('form') || document), 0);
  }, true);
  document.addEventListener('keydown', (e) => { if (e.key === 'Enter' && e.target.type === 'password') grab(e.target.form || document); }, true);
  window.__rbFill = (u, p) => {
    const pw = [...document.querySelectorAll('input[type=password]')].find(visible);
    if (!pw) return;
    const set = (el, v) => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(el, v);
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
    };
    const user = userFor(document, pw);
    if (user && u) set(user, u);
    set(pw, p);
  };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', scan); else scan();
  setTimeout(scan, 1500); setTimeout(scan, 4000);
})();"""
