# 002 — Слишком глубокое «нажатие» кнопок

**Флаги:** нет. Выполнять можно.

## Проблема
Правило KR-10 (hard): нажатая кнопка сжимается ровно до `scale(0.96)`. Сейчас глубже:
- `src/ui/style.css:113` `.icon-btn:active { background: var(--press); transform: scale(.88); }`
- `src/ui/style.css:288` `.tab .close:active, .tab .audio:active { transform: scale(.82); }`
- `src/ui/style.css:309` `#newtab:active { transform: rotate(90deg) scale(.85); }`
- `src/ui/style.css:982` `#engine:active { transform: scale(.95); }`
- `src/ui/style.css:1061` `.tile:active { transform: scale(.95); }`

## Цель
Везде `scale(.96)`; у `#newtab` поворот сохраняется: `rotate(90deg) scale(.96)`.

## Как принято в проекте
Уже правильно: `.text-btn:active { transform: scale(.96); }` (`style.css:127`), `.chip:active`, `.walltile:active`, `.setcat:active`.

## Шаги
1. В пяти строках выше заменить число внутри `scale(…)` на `.96`.
2. Найти в `src/ui/style.css` остальные `:active { … scale(` (поиск `:active.*scale`) и привести к `.96`, если значение другое.

## Не трогать
`scale` в анимациях появления (`@keyframes`), в `:hover`, у `.walltile`/`.chip`/`.text-btn`.

## Проверка
Поиск `:active[^{]*\{[^}]*scale\((?!\.96)` в `style.css` ничего не находит. Сквозные проверки `tests/e2e/run-e2e.ps1 -Release` проходят.
