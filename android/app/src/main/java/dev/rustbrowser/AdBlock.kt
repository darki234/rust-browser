package dev.rustbrowser

import android.content.Context
import android.net.Uri
import android.webkit.WebResourceResponse
import java.io.ByteArrayInputStream
import kotlin.concurrent.thread

/**
 * Блокировка рекламы по списку доменов (тот же blocklist.txt, что у настольной версии).
 * Список читается в фоне при запуске; проверка — в сетевом потоке WebView,
 * поэтому интерфейс от неё не тормозит никогда.
 */
object AdBlock {
    @Volatile private var domains: Set<String> = emptySet()

    fun load(context: Context) = thread(name = "adblock-list", priority = Thread.MIN_PRIORITY) {
        domains = context.assets.open("blocklist.txt").bufferedReader().useLines { lines ->
            lines.map { it.trim() }.filter { it.isNotEmpty() && !it.startsWith("#") }.toHashSet()
        }
    }

    /** Хост или любой его родительский домен в списке. */
    fun isBlocked(url: Uri): Boolean {
        var host = url.host?.lowercase() ?: return false
        val set = domains
        while (true) {
            if (host in set) return true
            val dot = host.indexOf('.')
            if (dot < 0) return false
            host = host.substring(dot + 1)
        }
    }

    /** Пустой ответ вместо рекламы: запрос даже не уходит в сеть. */
    fun emptyResponse() = WebResourceResponse("text/plain", "utf-8", 403, "Blocked", emptyMap(), ByteArrayInputStream(ByteArray(0)))
}
