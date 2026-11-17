package dev.rustbrowser.ui

import androidx.compose.ui.graphics.Color

/** Цвета — те же, что у настольной версии (style.css). */
data class Palette(
    val dark: Boolean,
    val bg: Color,
    val surface: Color,
    val surface2: Color,
    val text: Color,
    val muted: Color,
    val border: Color,
    val accent: Color,
    val onAccent: Color,
)

/** Акценты: светлый и тёмный вариант каждого. */
val ACCENTS = listOf(
    Color(0xFF4F5BF5) to Color(0xFF8288FF),
    Color(0xFFE04A9A) to Color(0xFFF06BB0),
    Color(0xFF0EA5B7) to Color(0xFF38CCDD),
    Color(0xFFF26B1D) to Color(0xFFFF8A45),
    Color(0xFF16A34A) to Color(0xFF34D399),
    Color(0xFF8B5CF6) to Color(0xFFA78BFA),
)

fun palette(dark: Boolean, amoled: Boolean, accent: Int): Palette {
    val (light, darkAccent) = ACCENTS.getOrElse(accent) { ACCENTS[0] }
    return when {
        dark && amoled -> Palette(true, Color.Black, Color(0xFF0D0D10), Color(0xFF16161B), Color(0xFFECEEF5), Color(0xFF9A9FB1),
            Color.White.copy(alpha = .10f), darkAccent, Color(0xFF10111A))
        dark -> Palette(true, Color(0xFF121318), Color(0xFF23242C), Color(0xFF1B1C22), Color(0xFFECEEF5), Color(0xFF9A9FB1),
            Color.White.copy(alpha = .08f), darkAccent, Color(0xFF10111A))
        else -> Palette(false, Color(0xFFF3F4F9), Color.White, Color(0xFFF0F1F6), Color(0xFF1B1C22), Color(0xFF697084),
            Color(0xFF141830).copy(alpha = .09f), light, Color.White)
    }
}

/** Оттенок цветной буквы-плитки — от имени сайта, как в настольной версии. */
fun hueOf(text: String): Float {
    var hash = 0L
    for (ch in text) hash = (hash * 31 + ch.code) and 0xFFFFFFFFL
    return (hash % 360).toFloat()
}
