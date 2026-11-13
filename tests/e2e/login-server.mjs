// Локальная страница входа для проверки менеджера паролей: http://localhost:<порт>/login
import { createServer } from 'node:http';

const port = Number(process.argv[2] || 8765);
const page = `<!doctype html><meta charset="utf-8"><title>Вход</title>
<style>body{font:16px system-ui;display:grid;place-items:center;height:90vh}form{display:grid;gap:10px;width:280px}</style>
<form action="/done" method="post">
  <h2>Вход в тестовый сайт</h2>
  <input id="user" name="login" type="email" autocomplete="username" placeholder="Почта">
  <input id="pass" name="password" type="password" autocomplete="current-password" placeholder="Пароль">
  <button id="go" type="submit">Войти</button>
</form>`;

createServer((req, res) => {
  res.setHeader('content-type', 'text/html; charset=utf-8');
  if (req.url.startsWith('/done')) return res.end('<!doctype html><meta charset="utf-8"><title>Готово</title><p>Вы вошли');
  res.end(page);
}).listen(port, '127.0.0.1', () => console.log(`login page on ${port}`));
