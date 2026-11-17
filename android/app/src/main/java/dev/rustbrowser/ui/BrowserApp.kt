package dev.rustbrowser.ui

import android.net.Uri
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import dev.rustbrowser.Bookmark
import dev.rustbrowser.Browser
import dev.rustbrowser.ENGINES
import dev.rustbrowser.MainActivity
import dev.rustbrowser.Screen
import dev.rustbrowser.SiteIcons
import dev.rustbrowser.Tab
import kotlinx.coroutines.delay
import kotlinx.coroutines.suspendCancellableCoroutine
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlin.coroutines.resume

/** Цвет острова в инкогнито — фиолетовый, чтобы режим было видно сразу. */
private val IncognitoSurface = Color(0xFF2A2240)

@Composable
fun BrowserApp(b: Browser, activity: MainActivity) {
    val store = b.store
    val systemDark = isSystemInDarkTheme()
    val dark = when (store.theme) { "light" -> false; "dark", "amoled" -> true; else -> systemDark }
    val p = palette(dark, store.theme == "amoled", store.accent)
    LaunchedEffect(dark, store.theme) {
        activity.applySystemBars()
        b.retheme(dark) // открытые сайты тоже узнают о смене темы
    }
    val tab = b.active

    Box(Modifier.fillMaxSize().background(p.bg)) {
        // страница: WebView активной вкладки; над стартовой — своя страница
        AndroidView(factory = { b.host }, modifier = Modifier.fillMaxSize().statusBarsPadding())
        if (tab == null || tab.isStart) StartPage(b, p)
        Island(b, p, Modifier.align(Alignment.BottomCenter))
        Overlays(b, p, activity)
    }
}

// ---------- остров ----------

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun Island(b: Browser, p: Palette, modifier: Modifier = Modifier) {
    val tab = b.active ?: return
    if (b.screen == Screen.Find) return
    val shown = b.islandShown || tab.isStart || b.screen != Screen.None
    val offset by animateDpAsState(if (shown) 0.dp else 120.dp, tween(280, easing = FastOutSlowInEasing), label = "island")
    val shape = RoundedCornerShape(28.dp)
    var drag by remember { mutableFloatStateOf(0f) }
    val surface = if (tab.incognito) IncognitoSurface else p.surface
    val ink = if (tab.incognito) Color(0xFFEDE7FF) else p.text

    Box(
        modifier
            .fillMaxWidth()
            .navigationBarsPadding()
            .padding(horizontal = 12.dp, vertical = 10.dp)
            .offset(y = offset)
            .graphicsLayer { translationX = drag * 0.3f }
            .shadow(18.dp, shape, ambientColor = Color.Black.copy(alpha = .3f), spotColor = Color.Black.copy(alpha = .4f))
            .clip(shape)
            .background(surface)
            .border(1.dp, p.border, shape)
            // смахивание острова вбок — соседняя вкладка, как в Safari
            .pointerInput(tab.id) {
                detectHorizontalDragGestures(
                    onDragEnd = { if (drag > 140) b.switchBy(-1) else if (drag < -140) b.switchBy(1); drag = 0f },
                    onDragCancel = { drag = 0f },
                ) { _, dx -> drag += dx }
            }
            .height(58.dp),
    ) {
        Row(Modifier.fillMaxSize().padding(horizontal = 5.dp), verticalAlignment = Alignment.CenterVertically) {
            IconBtn(RbIcons.back, "Назад", ink, enabled = !tab.isStart) { if (!b.back()) b.goHome() }
            AddressPill(tab, p, ink, Modifier.weight(1f)) { b.screen = Screen.Search }
            Box(
                Modifier.size(44.dp).clip(RoundedCornerShape(14.dp)).combinedClickable(
                    onClick = { b.capture(tab); b.showingIncognito = tab.incognito; b.screen = Screen.Tabs },
                    onLongClick = { b.newTab(incognito = tab.incognito) },
                ),
                contentAlignment = Alignment.Center,
            ) {
                val count = b.tabs.count { it.incognito == tab.incognito }
                Box(Modifier.size(23.dp).border(1.8.dp, ink, RoundedCornerShape(7.dp)), contentAlignment = Alignment.Center) {
                    Text(if (count > 99) "∞" else "$count", color = ink, fontSize = 11.sp, fontWeight = FontWeight.Bold)
                }
            }
            IconBtn(RbIcons.dots, "Меню", ink) { b.screen = Screen.Menu }
        }
        // полоска загрузки по нижнему краю острова
        if (tab.loading) {
            val progress by animateFloatAsState(tab.progress / 100f, tween(220), label = "progress")
            Box(
                Modifier.align(Alignment.BottomStart).fillMaxWidth(progress.coerceAtLeast(.05f)).height(2.5.dp)
                    .background(Brush.horizontalGradient(listOf(p.accent, Color(0xFFE04A9A), Color(0xFFFF7A3D)))),
            )
        }
    }
}

@Composable
private fun AddressPill(tab: Tab, p: Palette, ink: Color, modifier: Modifier, onClick: () -> Unit) {
    Row(
        modifier.height(42.dp).padding(horizontal = 4.dp).clip(RoundedCornerShape(21.dp))
            .background(if (tab.incognito) Color.White.copy(alpha = .08f) else p.surface2)
            .clickable(onClick = onClick).padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (tab.isStart) {
            Icon(RbIcons.search, null, Modifier.size(16.dp), tint = p.muted)
            Spacer(Modifier.width(8.dp))
            Text("Поиск или адрес", color = p.muted, fontSize = 15.sp, maxLines = 1)
        } else {
            val secure = tab.url.startsWith("https://")
            Icon(if (tab.incognito) RbIcons.mask else if (secure) RbIcons.lock else RbIcons.globe, null, Modifier.size(14.dp), tint = ink.copy(alpha = .6f))
            Spacer(Modifier.width(7.dp))
            Text(displayUrl(tab.url), color = ink, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

/** Что показать в адресной строке: сайт без www, а для поиска — сам запрос. */
fun displayUrl(url: String): String {
    val uri = Uri.parse(url)
    for ((_, _, base) in ENGINES) if (url.startsWith(base)) {
        val q = uri.getQueryParameter("q") ?: uri.getQueryParameter("text")
        if (!q.isNullOrBlank()) return q
    }
    return uri.host?.removePrefix("www.") ?: url
}

@Composable
fun IconBtn(icon: ImageVector, label: String, tint: Color, enabled: Boolean = true, size: Dp = 44.dp, onClick: () -> Unit) {
    Box(
        Modifier.size(size).clip(RoundedCornerShape(14.dp)).clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Icon(icon, label, Modifier.size(21.dp), tint = if (enabled) tint else tint.copy(alpha = .3f))
    }
}

// ---------- стартовая страница ----------

@OptIn(ExperimentalFoundationApi::class)
@Composable
fun StartPage(b: Browser, p: Palette) {
    var removing by remember { mutableStateOf<Bookmark?>(null) }
    // нажатия не проходят к странице под ней
    Box(Modifier.fillMaxSize().background(p.bg).clickable(MutableInteractionSource(), null) {}) {
        Aurora(p)
        Column(
            Modifier.fillMaxSize().statusBarsPadding().navigationBarsPadding().padding(horizontal = 22.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Spacer(Modifier.weight(.8f))
            Clock(p)
            Spacer(Modifier.height(26.dp))
            Row(
                Modifier.fillMaxWidth().height(54.dp).clip(RoundedCornerShape(27.dp))
                    .background(p.surface.copy(alpha = .86f)).border(1.dp, p.border, RoundedCornerShape(27.dp))
                    .clickable { b.screen = Screen.Search }.padding(horizontal = 18.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(RbIcons.search, null, Modifier.size(19.dp), tint = p.muted)
                Spacer(Modifier.width(12.dp))
                val engine = ENGINES.firstOrNull { it.first == b.store.engine }?.second ?: "Google"
                Text("Искать в $engine или ввести адрес", color = p.muted, fontSize = 15.sp, maxLines = 1)
            }
            Spacer(Modifier.height(30.dp))
            b.store.bookmarks.take(8).chunked(4).forEach { row ->
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
                    row.forEach { bm ->
                        Tile(bm, p, onClick = { b.navigate(bm.url) }, onLongClick = { removing = bm })
                    }
                    repeat(4 - row.size) { Spacer(Modifier.width(76.dp)) }
                }
                Spacer(Modifier.height(14.dp))
            }
            Spacer(Modifier.weight(1f))
            if (b.store.adblock && b.store.blockedTotal > 0) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(RbIcons.shield, null, Modifier.size(13.dp), tint = p.muted)
                    Spacer(Modifier.width(6.dp))
                    Text("Заблокировано рекламы и трекеров: ${b.store.blockedTotal}", color = p.muted, fontSize = 12.sp)
                }
            }
            Spacer(Modifier.height(92.dp)) // место под остров
        }
    }
    removing?.let { bm ->
        AlertDialog(
            onDismissRequest = { removing = null },
            title = { Text(bm.title) },
            text = { Text("Убрать сайт с новой вкладки и из закладок?") },
            confirmButton = { TextButton({ b.store.removeBookmark(bm); removing = null }) { Text("Убрать") } },
            dismissButton = { TextButton({ removing = null }) { Text("Отмена") } },
            containerColor = p.surface, titleContentColor = p.text, textContentColor = p.muted,
        )
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun Tile(bm: Bookmark, p: Palette, onClick: () -> Unit, onLongClick: () -> Unit) {
    Column(
        Modifier.width(76.dp).clip(RoundedCornerShape(18.dp)).combinedClickable(onClick = onClick, onLongClick = onLongClick).padding(vertical = 6.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box(
            Modifier.size(58.dp).clip(RoundedCornerShape(19.dp)).background(p.surface.copy(alpha = .86f))
                .border(1.dp, p.border, RoundedCornerShape(19.dp)),
            contentAlignment = Alignment.Center,
        ) {
            SiteIcon(bm.url, p, 30.dp)
        }
        Spacer(Modifier.height(6.dp))
        Text(bm.title, color = p.text, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** Значок сайта: из кэша или с сайта; пока его нет — цветная буква. */
@Composable
fun SiteIcon(url: String, p: Palette, size: Dp, known: ImageBitmap? = null) {
    val host = Uri.parse(url).host?.removePrefix("www.").orEmpty()
    val context = LocalContext.current
    val icon by produceState(known, host) {
        if (known == null && host.isNotEmpty()) value = suspendCancellableCoroutine { cont -> SiteIcons.load(context, host) { cont.resume(it) } }
    }
    val shown = known ?: icon
    if (shown != null) {
        Image(shown, null, Modifier.size(size).clip(RoundedCornerShape(size / 4)))
    } else {
        val hue = hueOf(host)
        Box(
            Modifier.size(size).clip(RoundedCornerShape(size / 3.4f)).background(Color.hsl(hue, .55f, if (p.dark) .48f else .52f)),
            contentAlignment = Alignment.Center,
        ) {
            Text((host.firstOrNull() ?: '?').uppercase(), color = Color.White, fontWeight = FontWeight.Bold, fontSize = (size.value * .5f).sp)
        }
    }
}

@Composable
private fun Clock(p: Palette) {
    var now by remember { mutableStateOf(Date()) }
    LaunchedEffect(Unit) {
        while (true) { now = Date(); delay(60_000 - System.currentTimeMillis() % 60_000) }
    }
    val time = remember(now) { SimpleDateFormat("HH:mm", Locale.getDefault()).format(now) }
    val date = remember(now) { SimpleDateFormat("EEEE, d MMMM", Locale("ru")).format(now).replaceFirstChar { it.uppercase() } }
    Text(time, color = p.text, fontSize = 76.sp, fontWeight = FontWeight.Light, letterSpacing = (-2).sp)
    Text(date, color = p.muted, fontSize = 15.sp)
}

/** Мягкие цветные пятна, как на новой вкладке настольной версии. Двигаются медленно:
 *  анимация читается только при рисовании, без перекомпоновки экрана. */
@Composable
private fun Aurora(p: Palette) {
    val t by rememberInfiniteTransition(label = "aurora").animateFloat(
        0f, 1f, infiniteRepeatable(tween(16000, easing = LinearEasing), RepeatMode.Reverse), label = "t",
    )
    val strength = if (p.dark) .30f else .20f
    Canvas(Modifier.fillMaxSize()) {
        val w = size.width; val h = size.height
        drawRect(Brush.radialGradient(listOf(p.accent.copy(alpha = strength), Color.Transparent),
            center = Offset(w * (.15f + .35f * t), h * .22f), radius = w * .95f))
        drawRect(Brush.radialGradient(listOf(Color(0xFFE04A9A).copy(alpha = strength * .9f), Color.Transparent),
            center = Offset(w * (.95f - .3f * t), h * (.62f + .1f * t)), radius = w * .85f))
        drawRect(Brush.radialGradient(listOf(Color(0xFF18B6C9).copy(alpha = strength * .7f), Color.Transparent),
            center = Offset(w * (.3f + .2f * t), h * .95f), radius = w * .7f))
    }
}
