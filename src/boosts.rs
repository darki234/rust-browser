//! «Бусты»: свой вид для отдельных сайтов — шрифт, цветной оттенок, скрытые
//! элементы и свой CSS. Хранятся в параметрах, применяются скриптом документа
//! до первой отрисовки страницы.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Boost {
    pub font: String,
    pub tint: String,
    pub hidden: Vec<String>,
    pub css: String,
}

impl Boost {
    pub fn is_empty(&self) -> bool {
        self.font.is_empty() && self.tint.is_empty() && self.hidden.is_empty() && self.css.trim().is_empty()
    }
}

/// Шрифты, которые есть в любой Windows 10/11.
pub const FONTS: [&str; 8] = ["Segoe UI Variable Text", "Georgia", "Cambria", "Calibri", "Bahnschrift", "Consolas", "Comic Sans MS", "Verdana"];
pub const TINTS: [&str; 8] = ["#3b82f6", "#8b5cf6", "#ec4899", "#ef4444", "#f97316", "#eab308", "#22c55e", "#06b6d4"];

/// Селектор из выбора элемента на странице: без фигурных скобок и тегов, разумной длины —
/// он попадает в CSS, и вырваться из правила не должен.
pub fn valid_selector(selector: &str) -> bool {
    let s = selector.trim();
    !s.is_empty() && s.len() <= 400 && !s.contains(['{', '}', '<', ';', '@', '\\', '\n'])
}

/// Свой CSS: длина ограничена, «</» убираем на всякий случай (стиль вставляется textContent).
pub fn clean_css(css: &str) -> String {
    css.chars().take(20_000).collect::<String>().replace("</", "<\\/")
}

/// Скрипт документа: находит буст для сайта страницы и вставляет стиль.
pub fn script(boosts: &BTreeMap<String, Boost>) -> String {
    let data = serde_json::to_string(boosts).unwrap_or_else(|_| "{}".into()).replace("</", "<\\/");
    format!(
        r#"(() => {{
  const B = {data};
  const host = location.hostname.replace(/^www\./, '');
  const b = B[host];
  const build = () => {{
    if (!b) return '';
    let css = '';
    if (b.font) css += '*:not(code):not(pre):not(kbd):not(samp):not(i):not([class*="icon"]):not([class*="Icon"]):not([class*="material"]){{font-family:"' + b.font + '",sans-serif!important}}';
    if (b.hidden && b.hidden.length) css += b.hidden.join(',') + '{{display:none!important}}';
    if (b.tint) css += 'html::after{{content:"";position:fixed;inset:0;background:' + b.tint + ';mix-blend-mode:color;opacity:.32;pointer-events:none;z-index:2147483647}}';
    if (b.css) css += b.css;
    return css;
  }};
  const apply = () => {{
    let style = document.getElementById('__rb_boost');
    const css = build();
    if (!css) {{ style?.remove(); return true; }}
    if (!document.documentElement) return false;
    if (!style) {{ style = document.createElement('style'); style.id = '__rb_boost'; document.documentElement.append(style); }}
    if (style.textContent !== css) style.textContent = css;
    return true;
  }};
  if (!apply()) new MutationObserver((_, o) => {{ if (apply()) o.disconnect(); }}).observe(document, {{ childList: true }});
  // сайты-приложения пересоздают <head> и могут выкинуть стиль — возвращаем
  if (b) addEventListener('load', () => setTimeout(apply, 300));
}})();"#
    )
}

/// Выбор элемента: подсветка под курсором, щелчок — скрыть, Esc — отмена.
pub const PICK_SCRIPT: &str = r#"(() => {
  if (window.__rbPick) return;
  window.__rbPick = true;
  const box = document.createElement('div');
  box.style.cssText = 'position:fixed;z-index:2147483647;pointer-events:none;border:2px solid #8b5cf6;background:rgba(139,92,246,.18);border-radius:4px;transition:all .06s';
  const tip = document.createElement('div');
  tip.textContent = 'Щёлкните элемент, который нужно скрыть · Esc — отмена';
  tip.style.cssText = 'position:fixed;z-index:2147483647;left:50%;top:16px;transform:translateX(-50%);padding:10px 16px;border-radius:12px;background:#1b1446;color:#fff;font:600 14px "Segoe UI",sans-serif;box-shadow:0 8px 30px rgba(0,0,0,.4);pointer-events:none';
  document.documentElement.append(box, tip);
  let target = null;
  const sel = (el) => {
    const css = (s) => (window.CSS && CSS.escape ? CSS.escape(s) : s);
    if (el.id && document.querySelectorAll('#' + css(el.id)).length === 1) return '#' + css(el.id);
    const parts = [];
    for (let node = el; node && node.nodeType === 1 && node !== document.documentElement && parts.length < 4; node = node.parentElement) {
      let part = node.tagName.toLowerCase();
      if (node.id && document.querySelectorAll('#' + css(node.id)).length === 1) { parts.unshift('#' + css(node.id)); break; }
      const classes = [...node.classList].filter((c) => /^[a-zA-Z][\w-]{1,40}$/.test(c)).slice(0, 2);
      if (classes.length) part += '.' + classes.map(css).join('.');
      const parent = node.parentElement;
      if (parent) {
        const same = [...parent.children].filter((c) => c.tagName === node.tagName);
        if (same.length > 1) part += ':nth-of-type(' + (same.indexOf(node) + 1) + ')';
      }
      parts.unshift(part);
    }
    return parts.join(' > ');
  };
  const stop = () => {
    window.__rbPick = false;
    box.remove(); tip.remove();
    removeEventListener('mousemove', move, true);
    removeEventListener('click', click, true);
    removeEventListener('keydown', key, true);
  };
  const move = (e) => {
    target = e.target;
    if (!target || target === box) return;
    const r = target.getBoundingClientRect();
    Object.assign(box.style, { left: r.left + 'px', top: r.top + 'px', width: r.width + 'px', height: r.height + 'px' });
  };
  const click = (e) => {
    e.preventDefault(); e.stopPropagation();
    if (target && target !== document.body && target !== document.documentElement) {
      const selector = sel(target);
      target.style.setProperty('display', 'none', 'important');
      try { window.chrome.webview.postMessage(JSON.stringify({ cmd: 'boost_hide', selector })); } catch (err) {}
    }
    stop();
  };
  const key = (e) => { if (e.key === 'Escape') { e.preventDefault(); stop(); } };
  addEventListener('mousemove', move, true);
  addEventListener('click', click, true);
  addEventListener('keydown', key, true);
})();"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selectors_cannot_break_out_of_css() {
        assert!(valid_selector("div.ad > span:nth-of-type(2)"));
        assert!(valid_selector("#banner"));
        assert!(!valid_selector("a{} body{display:none"));
        assert!(!valid_selector("</style><script>"));
        assert!(!valid_selector(""));
    }

    #[test]
    fn script_escapes_closing_tags() {
        let mut boosts = BTreeMap::new();
        boosts.insert("a.org".to_string(), Boost { css: clean_css("body{color:red}</style>"), ..Default::default() });
        let s = script(&boosts);
        assert!(!s.contains("</style>"));
        assert!(s.contains("a.org"));
    }
}
