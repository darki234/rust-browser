# 003 — Появление из нуля и закрытие с медленным стартом

**Флаги:** нет. Выполнять можно.

## Проблема
Детектор unslop (не связан с каноном контрольной суммой — это находки детектора):
- G-2: `src/ui/style.css:465` счётчик заблокированного (`.badge`) появляется с `transform: scale(0)`.
- G-1: `ease-in` у закрытия, видимая часть движения тянется:
  - `style.css:1177` `#layer.closing #hub { animation: hub-out .16s ease-in forwards; }`
  - `style.css:1458` `.menu.closing { animation: menu-out .12s ease-in forwards; }`
  - `style.css:1891–1892` заставка `s-compass` (`ease-in`).

## Цель
- `.badge` в скрытом состоянии: `transform: scale(.6); opacity: 0;`, в `.badge.show`: `transform: scale(1); opacity: 1;`, переход `transform .3s var(--spring), opacity .15s`.
- Закрытие хаба и меню: кривая `var(--ease)` вместо `ease-in`. Длительности не менять.
- Заставку `s-compass` не трогать (это намеренный выход заставки, KR-15).

## Как принято в проекте
Кривые — токены `--ease` и `--spring` в `:root` (`style.css:30–31`).

## Шаги
1. `style.css` около строки 465: в правиле `.badge` заменить `transform: scale(0);` на `transform: scale(.6); opacity: 0;`; в `transition` добавить `, opacity .15s`.
2. В `.badge.show` добавить `opacity: 1;`.
3. В строках 1177 и 1458 заменить `ease-in` на `var(--ease)`.

## Не трогать
`.badge.bump`, анимации открытия, заставку.

## Проверка
Счётчик рекламы на панели появляется без «выпрыгивания из точки»; меню и библиотека закрываются так же быстро. Сквозные проверки проходят (в них есть «меню», «параметры», «заставка»).
