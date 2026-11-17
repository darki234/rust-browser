package dev.rustbrowser

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors

/**
 * Значки сайтов для плиток. Сначала — тот, что WebView уже получил при открытии
 * сайта; иначе сами берём apple-touch-icon или favicon.ico с сайта. Всё лежит на диске,
 * повторно сеть не нужна. Память — только для показанных плиток.
 */
object SiteIcons {
    private val pool = Executors.newFixedThreadPool(2)
    private val memory = ConcurrentHashMap<String, ImageBitmap>()
    private val missing = ConcurrentHashMap.newKeySet<String>()

    private fun dir(context: Context) = File(context.filesDir, "site-icons").apply { mkdirs() }
    private fun key(host: String) = host.lowercase().removePrefix("www.")

    fun remember(context: Context, host: String, icon: Bitmap) {
        val k = key(host)
        val file = File(dir(context), "$k.png")
        // маленький значок не заменяет уже сохранённый крупный
        val old = memory[k]
        if (old != null && old.width >= icon.width) return
        memory[k] = icon.asImageBitmap()
        missing.remove(k)
        pool.execute { runCatching { file.outputStream().use { icon.compress(Bitmap.CompressFormat.PNG, 100, it) } } }
    }

    /** Значок с диска или из сети; результат приходит в [done] в фоновом потоке. */
    fun load(context: Context, host: String, done: (ImageBitmap?) -> Unit) {
        val k = key(host)
        memory[k]?.let { return done(it) }
        if (k in missing) return done(null)
        pool.execute {
            val file = File(dir(context), "$k.png")
            val bitmap = file.takeIf { it.exists() }?.let { BitmapFactory.decodeFile(it.path) }
                ?: fetch(k)?.also { b -> runCatching { file.outputStream().use { b.compress(Bitmap.CompressFormat.PNG, 100, it) } } }
            val image = bitmap?.asImageBitmap()
            if (image != null) memory[k] = image else missing += k
            done(image)
        }
    }

    private fun fetch(host: String): Bitmap? {
        for (path in listOf("/apple-touch-icon.png", "/favicon.ico")) {
            for (h in listOf(host, "www.$host")) {
                val bitmap = runCatching {
                    val c = URL("https://$h$path").openConnection() as HttpURLConnection
                    c.connectTimeout = 4000; c.readTimeout = 4000; c.instanceFollowRedirects = true
                    c.inputStream.use { BitmapFactory.decodeStream(it) }.also { c.disconnect() }
                }.getOrNull()
                if (bitmap != null && bitmap.width >= 16) return bitmap
            }
        }
        return null
    }
}
