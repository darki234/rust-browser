package dev.rustbrowser.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Icon
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.rustbrowser.Browser
import dev.rustbrowser.ENGINES
import dev.rustbrowser.MainActivity
import dev.rustbrowser.Screen
import dev.rustbrowser.Tab
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import kotlin.math.abs
import kotlin.math.roundToInt

@Composable
fun BoxScope.Overlays(b: Browser, p: Palette, activity: MainActivity) {
    AnimatedVisibility(b.screen == Screen.Search, enter = fadeIn(tween(150)) + slideInVertically(tween(220)) { it / 10 }, exit = fadeOut(tween(110))) {
        SearchScreen(b, p)
    }
    AnimatedVisibility(b.screen == Screen.Tabs, enter = fadeIn(tween(160)) + scaleIn(tween(220), initialScale = .94f), exit = fadeOut(tween(130)) + scaleOut(tween(160), targetScale = .96f)) {
        TabsScreen(b, p)
    }
    // меню — затемнение и лист снизу
    AnimatedVisibility(b.screen == Screen.Menu, enter = fadeIn(tween(160)), exit = fadeOut(tween(160))) {
        Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = .38f)).clickable(MutableInteractionSource(), null) { b.screen = Screen.None })
    }
    AnimatedVisibility(
        b.screen == Screen.Menu, modifier = Modifier.align(Alignment.BottomCenter),
        enter = slideInVertically(tween(260)) { it } + fadeIn(tween(160)), exit = slideOutVertically(tween(200)) { it } + fadeOut(tween(160)),
    ) { MenuSheet(b, p) }
    for ((screen, title) in listOf(Screen.Bookmarks to "Закладки", Screen.History to "История", Screen.Settings to "Настройки")) {
        AnimatedVisibility(b.screen == screen, enter = slideInHorizontally(tween(240)) { it }, exit = slideOutHorizontally(tween(200)) { it }) {
            Panel(title, p, onBack = { b.screen = Screen.None }) {
                when (screen) {
                    Screen.Bookmarks -> BookmarksList(b, p)
                    Screen.History -> HistoryList(b, p)
                    else -> SettingsList(b, p, activity)
                }
            }
        }
    }
    AnimatedVisibility(b.screen == Screen.Find, modifier = Modifier.align(Alignment.BottomCenter), enter = slideInVertically { it }, exit = slideOutVertically { it }) {
        FindBar(b, p)
    }
}

// ---------- поиск и адрес ----------

private data class Suggestion(val title: String, val url: String, val star: Boolean)

@Composable
private fun SearchScreen(b: Browser, p: Palette) {
    val tab = b.active
    val initial = tab?.url?.takeIf { !tab.isStart }.orEmpty()
    var query by remember { mutableStateOf(TextFieldValue(initial, TextRange(0, initial.length))) }
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    val q = query.text.trim()
    val suggestions = remember(q) {
        val needle = q.lowercase()
        val list = b.store.bookmarks.map { Suggestion(it.title, it.url, true) } + b.store.history.map { Suggestion(it.title, it.url, false) }
        list.asSequence()
            .filter { needle.isEmpty() || it.url.lowercase().contains(needle) || it.title.lowercase().contains(needle) }
            .distinctBy { it.url }.take(if (needle.isEmpty()) 6 else 8).toList()
    }
    val engine = ENGINES.firstOrNull { it.first == b.store.engine }?.second ?: "Google"
    Column(Modifier.fillMaxSize().background(p.bg).statusBarsPadding().navigationBarsPadding().imePadding()) {
        LazyColumn(Modifier.weight(1f).fillMaxWidth(), reverseLayout = true, contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp)) {
            if (q.isNotEmpty() && q != initial) item {
                Row2(RbIcons.search, "Искать «$q»", engine, p, accent = true) { b.navigate(q) }
            }
            items(suggestions, key = { it.url }) { s ->
                Row2(if (s.star) RbIcons.star else RbIcons.clock, s.title.ifBlank { s.url }, displayUrl(s.url), p) { b.navigate(s.url) }
            }
            if (q.isEmpty() && suggestions.isNotEmpty()) item {
                Text("Недавние и закладки", color = p.muted, fontSize = 12.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(8.dp))
            }
        }
        // поле ввода — внизу, у большого пальца
        Row(
            Modifier.padding(12.dp).fillMaxWidth().height(56.dp).clip(RoundedCornerShape(28.dp))
                .background(p.surface).border(1.5.dp, p.accent.copy(alpha = .7f), RoundedCornerShape(28.dp)).padding(start = 18.dp, end = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(RbIcons.search, null, Modifier.size(18.dp), tint = p.muted)
            Spacer(Modifier.width(10.dp))
            Box(Modifier.weight(1f)) {
                if (query.text.isEmpty()) Text("Поиск или адрес", color = p.muted, fontSize = 16.sp)
                BasicTextField(
                    query, { query = it },
                    Modifier.fillMaxWidth().focusRequester(focus)
                        // Enter с аппаратной клавиатуры (планшет, Chromebook) — как кнопка «Перейти»
                        .onPreviewKeyEvent {
                            if (it.key == Key.Enter && it.type == KeyEventType.KeyUp) { b.navigate(query.text); true } else false
                        },
                    singleLine = true,
                    textStyle = TextStyle(color = p.text, fontSize = 16.sp),
                    cursorBrush = SolidColor(p.accent),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go, autoCorrectEnabled = false),
                    keyboardActions = KeyboardActions(onGo = { b.navigate(query.text) }),
                )
            }
            if (query.text.isNotEmpty()) IconBtn(RbIcons.close, "Стереть", p.muted, size = 40.dp) { query = TextFieldValue("") }
            Text("Отмена", color = p.accent, fontSize = 15.sp, fontWeight = FontWeight.SemiBold,
                modifier = Modifier.clip(RoundedCornerShape(16.dp)).clickable { b.screen = Screen.None }.padding(horizontal = 12.dp, vertical = 10.dp))
        }
    }
}

@Composable
private fun Row2(icon: ImageVector, title: String, subtitle: String, p: Palette, accent: Boolean = false, trailing: (@Composable () -> Unit)? = null, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).clickable(onClick = onClick).padding(horizontal = 10.dp, vertical = 9.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(36.dp).clip(RoundedCornerShape(12.dp)).background(if (accent) p.accent.copy(alpha = .16f) else p.surface2), contentAlignment = Alignment.Center) {
            Icon(icon, null, Modifier.size(17.dp), tint = if (accent) p.accent else p.muted)
        }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(title, color = p.text, fontSize = 15.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (subtitle.isNotEmpty()) Text(subtitle, color = p.muted, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        trailing?.invoke()
    }
}

// ---------- вкладки ----------

@Composable
private fun TabsScreen(b: Browser, p: Palette) {
    val list = b.tabs.filter { it.incognito == b.showingIncognito }
    Column(Modifier.fillMaxSize().background(p.bg).statusBarsPadding().navigationBarsPadding()) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 18.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("Вкладки", color = p.text, fontSize = 24.sp, fontWeight = FontWeight.SemiBold)
            Spacer(Modifier.weight(1f))
            Segmented(
                listOf("Обычные ${b.tabs.count { !it.incognito }}", "Инкогнито ${b.tabs.count { it.incognito }}"),
                if (b.showingIncognito) 1 else 0, p,
            ) { b.showingIncognito = it == 1 }
        }
        if (list.isEmpty()) {
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Icon(if (b.showingIncognito) RbIcons.mask else RbIcons.tabs, null, Modifier.size(42.dp), tint = p.muted)
                    Spacer(Modifier.height(10.dp))
                    Text(
                        if (b.showingIncognito) "Инкогнито не сохраняет историю,\ncookie и данные сайтов" else "Нет открытых вкладок",
                        color = p.muted, fontSize = 14.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                    )
                }
            }
        } else {
            LazyVerticalGrid(
                GridCells.Fixed(2), Modifier.weight(1f), contentPadding = PaddingValues(12.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                items(list, key = { it.id }) { tab ->
                    TabCard(tab, tab.id == b.activeId, p, Modifier.animateItem(),
                        onOpen = { b.activate(tab); b.screen = Screen.None },
                        onClose = { b.closeTab(tab) })
                }
            }
        }
        Row(Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("Закрыть все", color = if (list.isEmpty()) p.muted else p.text, fontSize = 15.sp,
                modifier = Modifier.clip(RoundedCornerShape(16.dp)).clickable(enabled = list.isNotEmpty()) { b.closeAll(b.showingIncognito) }.padding(12.dp))
            Spacer(Modifier.weight(1f))
            Box(
                Modifier.size(58.dp).clip(CircleShape)
                    .background(Brush.linearGradient(listOf(p.accent, Color(0xFFE04A9A))))
                    .clickable { b.newTab(incognito = b.showingIncognito); b.screen = Screen.None },
                contentAlignment = Alignment.Center,
            ) { Icon(RbIcons.plus, "Новая вкладка", Modifier.size(26.dp), tint = Color.White) }
            Spacer(Modifier.weight(1f))
            Text("Готово", color = p.accent, fontSize = 15.sp, fontWeight = FontWeight.SemiBold,
                modifier = Modifier.clip(RoundedCornerShape(16.dp)).clickable { b.screen = Screen.None }.padding(12.dp))
        }
    }
}

@Composable
private fun TabCard(tab: Tab, active: Boolean, p: Palette, modifier: Modifier, onOpen: () -> Unit, onClose: () -> Unit) {
    // смахивание карточки в сторону закрывает вкладку
    var dx by remember { mutableFloatStateOf(0f) }
    val shape = RoundedCornerShape(20.dp)
    Column(
        modifier
            .offset { IntOffset(dx.roundToInt(), 0) }
            .alpha(1f - (abs(dx) / 600f).coerceIn(0f, .7f))
            .pointerInput(tab.id) {
                detectHorizontalDragGestures(onDragEnd = { if (abs(dx) > 220) onClose() else dx = 0f }, onDragCancel = { dx = 0f }) { _, d -> dx += d }
            }
            .clip(shape).background(p.surface)
            .border(if (active) 2.dp else 1.dp, if (active) p.accent else p.border, shape)
            .clickable(onClick = onOpen),
    ) {
        Row(Modifier.fillMaxWidth().height(38.dp).padding(start = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            if (tab.isStart) Icon(RbIcons.search, null, Modifier.size(15.dp), tint = p.muted) else SiteIcon(tab.url, p, 16.dp, tab.favicon)
            Spacer(Modifier.width(7.dp))
            Text(if (tab.isStart) "Новая вкладка" else tab.title.ifBlank { displayUrl(tab.url) }, color = p.text, fontSize = 12.sp,
                maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
            IconBtn(RbIcons.close, "Закрыть", p.muted, size = 36.dp, onClick = onClose)
        }
        Box(Modifier.fillMaxWidth().aspectRatio(.74f).background(p.surface2)) {
            val thumb = tab.thumbnail
            if (thumb != null && !tab.isStart) {
                Image(thumb, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop, alignment = Alignment.TopCenter)
            } else {
                Box(Modifier.fillMaxSize().background(Brush.linearGradient(listOf(p.accent.copy(alpha = .25f), Color(0xFFE04A9A).copy(alpha = .18f)))), contentAlignment = Alignment.Center) {
                    if (tab.isStart) Icon(RbIcons.search, null, Modifier.size(30.dp), tint = p.text.copy(alpha = .5f))
                    else SiteIcon(tab.url, p, 40.dp, tab.favicon)
                }
            }
        }
    }
}

@Composable
private fun Segmented(options: List<String>, selected: Int, p: Palette, onPick: (Int) -> Unit) {
    Row(Modifier.clip(RoundedCornerShape(18.dp)).background(p.surface2).padding(3.dp)) {
        options.forEachIndexed { i, label ->
            Text(label, color = if (i == selected) p.onAccent else p.muted, fontSize = 13.sp, fontWeight = FontWeight.SemiBold,
                modifier = Modifier.clip(RoundedCornerShape(15.dp)).background(if (i == selected) p.accent else Color.Transparent)
                    .clickable { onPick(i) }.padding(horizontal = 12.dp, vertical = 7.dp))
        }
    }
}

// ---------- меню ----------

@Composable
private fun MenuSheet(b: Browser, p: Palette) {
    val tab = b.active ?: return
    val page = !tab.isStart
    Column(
        Modifier.fillMaxWidth().navigationBarsPadding().padding(10.dp).clip(RoundedCornerShape(30.dp))
            .background(p.surface).border(1.dp, p.border, RoundedCornerShape(30.dp)).padding(14.dp),
    ) {
        Box(Modifier.align(Alignment.CenterHorizontally).size(36.dp, 4.dp).clip(CircleShape).background(p.muted.copy(alpha = .35f)))
        Spacer(Modifier.height(12.dp))
        if (page) {
            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(horizontal = 4.dp)) {
                SiteIcon(tab.url, p, 28.dp, tab.favicon)
                Spacer(Modifier.width(10.dp))
                Column(Modifier.weight(1f)) {
                    Text(tab.title.ifBlank { displayUrl(tab.url) }, color = p.text, fontSize = 15.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(displayUrl(tab.url) + if (tab.blocked > 0) " · заблокировано ${tab.blocked}" else "", color = p.muted, fontSize = 12.sp, maxLines = 1)
                }
                val starred = b.store.isBookmarked(tab.url)
                IconBtn(RbIcons.star, "В закладки", if (starred) Color(0xFFF5B83D) else p.muted) { b.store.toggleBookmark(tab.url, tab.title) }
            }
            Spacer(Modifier.height(12.dp))
        }
        val items = listOf(
            MenuAction(RbIcons.forward, "Вперёд", tab.canForward) { b.forward(); b.screen = Screen.None },
            MenuAction(RbIcons.reload, if (tab.loading) "Стоп" else "Обновить", page) { b.reloadOrStop(); b.screen = Screen.None },
            MenuAction(RbIcons.plus, "Вкладка") { b.newTab(); b.screen = Screen.None },
            MenuAction(RbIcons.mask, "Инкогнито") { b.newTab(incognito = true); b.screen = Screen.None },
            MenuAction(RbIcons.library, "Закладки") { b.screen = Screen.Bookmarks },
            MenuAction(RbIcons.clock, "История") { b.screen = Screen.History },
            MenuAction(RbIcons.share, "Поделиться", page) { b.screen = Screen.None; b.share() },
            MenuAction(RbIcons.search, "Найти", page) { b.screen = Screen.Find },
            MenuAction(RbIcons.desktop, "Для ПК", page, on = tab.desktop) { b.toggleDesktop(); b.screen = Screen.None },
            MenuAction(RbIcons.moon, "Тёмные сайты", on = b.store.darkSites) { b.store.darkSites = !b.store.darkSites; b.applyDarkSites(); b.store.saveSettings() },
            MenuAction(RbIcons.shield, "Реклама", on = b.store.adblock) { b.store.adblock = !b.store.adblock; b.store.saveSettings() },
            MenuAction(RbIcons.gear, "Настройки") { b.screen = Screen.Settings },
        )
        items.chunked(4).forEach { row ->
            Row(Modifier.fillMaxWidth()) { row.forEach { MenuTile(it, p, Modifier.weight(1f)) } }
        }
    }
}

private class MenuAction(val icon: ImageVector, val label: String, val enabled: Boolean = true, val on: Boolean? = null, val action: () -> Unit)

@Composable
private fun MenuTile(a: MenuAction, p: Palette, modifier: Modifier) {
    Column(
        modifier.clip(RoundedCornerShape(18.dp)).clickable(enabled = a.enabled, onClick = a.action).padding(vertical = 10.dp).alpha(if (a.enabled) 1f else .35f),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        val lit = a.on == true
        Box(
            Modifier.size(48.dp).clip(RoundedCornerShape(16.dp)).background(if (lit) p.accent else p.surface2),
            contentAlignment = Alignment.Center,
        ) { Icon(a.icon, a.label, Modifier.size(21.dp), tint = if (lit) p.onAccent else p.text) }
        Spacer(Modifier.height(6.dp))
        Text(a.label, color = p.text, fontSize = 11.5.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

// ---------- закладки, история, настройки ----------

@Composable
private fun Panel(title: String, p: Palette, onBack: () -> Unit, content: @Composable () -> Unit) {
    Column(Modifier.fillMaxSize().background(p.bg).statusBarsPadding().navigationBarsPadding()) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 6.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
            IconBtn(RbIcons.back, "Назад", p.text, onClick = onBack)
            Text(title, color = p.text, fontSize = 22.sp, fontWeight = FontWeight.SemiBold)
        }
        Box(Modifier.weight(1f)) { content() }
    }
}

@Composable
private fun BookmarksList(b: Browser, p: Palette) {
    if (b.store.bookmarks.isEmpty()) return Empty("Закладок пока нет — звёздочка в меню добавит сайт", p)
    LazyColumn(contentPadding = PaddingValues(12.dp)) {
        items(b.store.bookmarks, key = { it.url }) { bm ->
            Row2(RbIcons.star, bm.title, displayUrl(bm.url), p,
                trailing = { IconBtn(RbIcons.trash, "Удалить", p.muted, size = 40.dp) { b.store.removeBookmark(bm) } }) { b.navigate(bm.url) }
        }
    }
}

@Composable
private fun HistoryList(b: Browser, p: Palette) {
    if (b.store.history.isEmpty()) return Empty("История пуста", p)
    val day = remember { SimpleDateFormat("d MMMM", Locale("ru")) }
    val time = remember { SimpleDateFormat("HH:mm", Locale.getDefault()) }
    LazyColumn(contentPadding = PaddingValues(12.dp)) {
        item {
            Text("Очистить историю", color = Color(0xFFE5484D), fontSize = 15.sp, fontWeight = FontWeight.SemiBold,
                modifier = Modifier.clip(RoundedCornerShape(14.dp)).clickable { b.store.clearHistory() }.padding(12.dp))
        }
        var lastDay = ""
        b.store.history.take(500).forEach { v ->
            val d = day.format(Date(v.time))
            if (d != lastDay) { lastDay = d; item { Text(d, color = p.muted, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(start = 10.dp, top = 14.dp, bottom = 4.dp)) } }
            item { Row2(RbIcons.clock, v.title.ifBlank { v.url }, time.format(Date(v.time)) + " · " + displayUrl(v.url), p) { b.navigate(v.url) } }
        }
    }
}

@Composable
private fun Empty(text: String, p: Palette) {
    Box(Modifier.fillMaxSize().padding(32.dp), contentAlignment = Alignment.Center) { Text(text, color = p.muted, fontSize = 14.sp) }
}

@Composable
private fun SettingsList(b: Browser, p: Palette, activity: MainActivity) {
    val s = b.store
    LazyColumn(contentPadding = PaddingValues(14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        item {
            Card(p, "Оформление") {
                Label("Тема", p)
                Chips(listOf("auto" to "Как в системе", "light" to "Светлая", "dark" to "Тёмная", "amoled" to "AMOLED"), s.theme, p) { s.theme = it; s.saveSettings() }
                Spacer(Modifier.height(14.dp))
                Label("Цвет", p)
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    ACCENTS.forEachIndexed { i, (light, dark) ->
                        val c = if (p.dark) dark else light
                        Box(
                            Modifier.size(36.dp).clip(CircleShape).background(c)
                                .border(3.dp, if (s.accent == i) p.text else Color.Transparent, CircleShape)
                                .clickable { s.accent = i; s.saveSettings() },
                        )
                    }
                }
                Spacer(Modifier.height(6.dp))
                Toggle("Тёмная тема для сайтов", "Сайты без своей тёмной темы перекрашивает сам движок; уже тёмные не трогает", s.darkSites, p) {
                    s.darkSites = it; b.applyDarkSites(); s.saveSettings()
                }
            }
        }
        item {
            Card(p, "Поиск") {
                Chips(ENGINES.map { it.first to it.second }, s.engine, p) { s.engine = it; s.saveSettings() }
            }
        }
        item {
            Card(p, "Конфиденциальность") {
                Toggle("Блокировка рекламы и трекеров", "Около 3 600 рекламных доменов · заблокировано: ${s.blockedTotal}", s.adblock, p) { s.adblock = it; s.saveSettings() }
                Action("Очистить историю", p) { s.clearHistory() }
                Action("Удалить cookie и данные сайтов", p) { b.clearData() }
            }
        }
        item {
            Card(p, "Браузер") {
                Action(if (activity.isDefaultBrowser()) "Rust Browser — браузер по умолчанию ✓" else "Сделать браузером по умолчанию", p) { activity.requestDefaultBrowser() }
                Text("Rust Browser ${activity.packageManager.getPackageInfo(activity.packageName, 0).versionName} · Android WebView", color = p.muted, fontSize = 12.sp, modifier = Modifier.padding(top = 8.dp))
            }
        }
    }
}

@Composable
private fun Card(p: Palette, title: String, content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(22.dp)).background(p.surface).border(1.dp, p.border, RoundedCornerShape(22.dp)).padding(16.dp)) {
        Text(title, color = p.text, fontSize = 16.sp, fontWeight = FontWeight.SemiBold)
        Spacer(Modifier.height(12.dp))
        content()
    }
}

@Composable
private fun Label(text: String, p: Palette) = Text(text, color = p.muted, fontSize = 13.sp, modifier = Modifier.padding(bottom = 8.dp))

@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
private fun Chips(options: List<Pair<String, String>>, selected: String, p: Palette, onPick: (String) -> Unit) {
    // переносятся на новую строку, если не помещаются
    androidx.compose.foundation.layout.FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        options.forEach { (id, label) ->
            val on = id == selected
            Text(label, color = if (on) p.onAccent else p.text, fontSize = 14.sp,
                modifier = Modifier.clip(RoundedCornerShape(16.dp)).background(if (on) p.accent else p.surface2).clickable { onPick(id) }.padding(horizontal = 14.dp, vertical = 9.dp))
        }
    }
}

@Composable
private fun Toggle(title: String, hint: String, value: Boolean, p: Palette, onChange: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).clickable { onChange(!value) }.padding(vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(title, color = p.text, fontSize = 15.sp)
            Text(hint, color = p.muted, fontSize = 12.sp)
        }
        Spacer(Modifier.width(10.dp))
        Switch(value, onChange, colors = SwitchDefaults.colors(checkedTrackColor = p.accent, checkedThumbColor = p.onAccent, uncheckedTrackColor = p.surface2, uncheckedBorderColor = p.border))
    }
}

@Composable
private fun Action(text: String, p: Palette, onClick: () -> Unit) {
    Text(text, color = p.accent, fontSize = 15.sp, fontWeight = FontWeight.Medium,
        modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).clickable(onClick = onClick).padding(vertical = 12.dp))
}

// ---------- поиск по странице ----------

@Composable
private fun FindBar(b: Browser, p: Palette) {
    var query by remember { mutableStateOf("") }
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    Row(
        Modifier.fillMaxWidth().navigationBarsPadding().imePadding().padding(12.dp).height(56.dp).clip(RoundedCornerShape(28.dp))
            .background(p.surface).border(1.dp, p.border, RoundedCornerShape(28.dp)).padding(start = 18.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.weight(1f)) {
            if (query.isEmpty()) Text("Найти на странице", color = p.muted, fontSize = 16.sp)
            BasicTextField(query, { query = it; b.find(it) }, Modifier.fillMaxWidth().focusRequester(focus), singleLine = true,
                textStyle = TextStyle(color = p.text, fontSize = 16.sp), cursorBrush = SolidColor(p.accent),
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search), keyboardActions = KeyboardActions(onSearch = { b.findNext(true) }))
        }
        val (current, total) = b.findCount
        if (query.isNotEmpty()) Text("$current/$total", color = p.muted, fontSize = 13.sp, modifier = Modifier.padding(horizontal = 6.dp))
        IconBtn(RbIcons.up, "Предыдущее", p.text, size = 40.dp) { b.findNext(false) }
        IconBtn(RbIcons.down, "Следующее", p.text, size = 40.dp) { b.findNext(true) }
        IconBtn(RbIcons.close, "Закрыть", p.text, size = 40.dp) { b.findClose() }
    }
}
