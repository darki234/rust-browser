package dev.rustbrowser.ui

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.addPathNodes
import androidx.compose.ui.unit.dp

/** Значки — те же контурные SVG, что у настольной версии (сгенерировано из app.js). */
object RbIcons {
    private fun icon(strokes: List<String>, fills: List<String>): ImageVector {
        val b = ImageVector.Builder(defaultWidth = 24.dp, defaultHeight = 24.dp, viewportWidth = 24f, viewportHeight = 24f)
        strokes.forEach {
            b.addPath(addPathNodes(it), stroke = SolidColor(Color.Black), strokeLineWidth = 2f,
                strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round)
        }
        fills.forEach { b.addPath(addPathNodes(it), fill = SolidColor(Color.Black)) }
        return b.build()
    }

    val back = icon(listOf("M19 12H5", "m12 19-7-7 7-7"), listOf())
    val forward = icon(listOf("M5 12h14", "m12 5 7 7-7 7"), listOf())
    val reload = icon(listOf("M21 12a9 9 0 1 1-2.64-6.36L21 8", "M21 3v5h-5"), listOf())
    val close = icon(listOf("M18 6 6 18", "m6 6 12 12"), listOf())
    val plus = icon(listOf("M12 5v14", "M5 12h14"), listOf())
    val star = icon(listOf("m12 2 3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01z"), listOf())
    val shield = icon(listOf("M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"), listOf())
    val download = icon(listOf("M12 4v11", "m7 11 5 5 5-5", "M5 20h14"), listOf())
    val clock = icon(listOf("M2.5,12.0a9.5,9.5 0 1,0 19.0,0a9.5,9.5 0 1,0 -19.0,0", "M12 7v5l3 2"), listOf())
    val lock = icon(listOf("M6.0,11.0h12.0a2.0,2.0 0 0 1 2.0,2.0v6.0a2.0,2.0 0 0 1 -2.0,2.0h-12.0a2.0,2.0 0 0 1 -2.0,-2.0v-6.0a2.0,2.0 0 0 1 2.0,-2.0z", "M8 11V7a4 4 0 0 1 8 0v4"), listOf())
    val search = icon(listOf("M4.0,11.0a7.0,7.0 0 1,0 14.0,0a7.0,7.0 0 1,0 -14.0,0", "m20.5 20.5-4.5-4.5"), listOf())
    val globe = icon(listOf("M2.5,12.0a9.5,9.5 0 1,0 19.0,0a9.5,9.5 0 1,0 -19.0,0", "M2.5 12h19", "M12 2.5c3 3 3 16 0 19", "M12 2.5c-3 3-3 16 0 19"), listOf())
    val trash = icon(listOf("M4 7h16", "M18 7v12a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V7", "M9 7V4h6v3"), listOf())
    val check = icon(listOf("m5 12.5 4.5 4.5L19 7.5"), listOf())
    val up = icon(listOf("m6 15 6-6 6 6"), listOf())
    val down = icon(listOf("m6 9 6 6 6-6"), listOf())
    val gear = icon(listOf("M9.0,12.0a3.0,3.0 0 1,0 6.0,0a3.0,3.0 0 1,0 -6.0,0", "M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"), listOf())
    val mask = icon(listOf("M3 10c0-2 1.5-3 4-3h10c2.5 0 4 1 4 3 0 4-2.5 7-5 7-1.6 0-2.4-1.5-4-1.5S9.6 17 8 17c-2.5 0-5-3-5-7z"), listOf("M7.1,11.0a1.4,1.4 0 1,0 2.8,0a1.4,1.4 0 1,0 -2.8,0", "M14.1,11.0a1.4,1.4 0 1,0 2.8,0a1.4,1.4 0 1,0 -2.8,0"))
    val sun = icon(listOf("M8.0,12.0a4.0,4.0 0 1,0 8.0,0a4.0,4.0 0 1,0 -8.0,0", "M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"), listOf())
    val moon = icon(listOf("M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z"), listOf())
    val home = icon(listOf("M3 11 12 4l9 7", "M5 10v10h14V10", "M10 20v-6h4v6"), listOf())
    val dots = icon(listOf(), listOf("M3.7,12.0a1.3,1.3 0 1,0 2.6,0a1.3,1.3 0 1,0 -2.6,0", "M10.7,12.0a1.3,1.3 0 1,0 2.6,0a1.3,1.3 0 1,0 -2.6,0", "M17.7,12.0a1.3,1.3 0 1,0 2.6,0a1.3,1.3 0 1,0 -2.6,0"))
    val library = icon(listOf("M4 4v16", "M9 4v16", "m14 5 5 15"), listOf())
    val window = icon(listOf("M5.0,5.0h14.0a2.0,2.0 0 0 1 2.0,2.0v11.0a2.0,2.0 0 0 1 -2.0,2.0h-14.0a2.0,2.0 0 0 1 -2.0,-2.0v-11.0a2.0,2.0 0 0 1 2.0,-2.0z", "M3 9h18"), listOf())
    val copy = icon(listOf("M10.0,8.0h8.0a2.0,2.0 0 0 1 2.0,2.0v8.0a2.0,2.0 0 0 1 -2.0,2.0h-8.0a2.0,2.0 0 0 1 -2.0,-2.0v-8.0a2.0,2.0 0 0 1 2.0,-2.0z", "M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"), listOf())
    val share = icon(listOf("M12 3v12", "m7 8 5-5 5 5", "M5 13v6a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-6"), listOf())
    val desktop = icon(listOf("M4.5 4h15a1.5 1.5 0 0 1 1.5 1.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 14.5v-9A1.5 1.5 0 0 1 4.5 4z", "M8 20h8", "M12 16v4"), listOf())
    val tabs = icon(listOf("M7.5 3.5h11a2 2 0 0 1 2 2v11", "M5.5 7.5h10a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-10a2 2 0 0 1-2-2v-9a2 2 0 0 1 2-2z"), listOf())
}
