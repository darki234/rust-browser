# Рисует значок браузера и сохраняет assets\icon.ico (размеры 16–256) и assets\icon.png.
# Запуск: powershell -File assets\make-icon.ps1
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.IO;

public static class IconMaker
{
    // Компас на градиентном скруглённом квадрате. Все размеры заданы в долях стороны.
    public static Bitmap Draw(int n)
    {
        var bmp = new Bitmap(n, n, PixelFormat.Format32bppArgb);
        using (var g = Graphics.FromImage(bmp))
        {
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.PixelOffsetMode = PixelOffsetMode.HighQuality;
            g.Clear(Color.Transparent);

            float pad = n * 0.04f, side = n - 2 * pad, r = side * 0.235f;
            using (var path = new GraphicsPath())
            {
                path.AddArc(pad, pad, 2 * r, 2 * r, 180, 90);
                path.AddArc(pad + side - 2 * r, pad, 2 * r, 2 * r, 270, 90);
                path.AddArc(pad + side - 2 * r, pad + side - 2 * r, 2 * r, 2 * r, 0, 90);
                path.AddArc(pad, pad + side - 2 * r, 2 * r, 2 * r, 90, 90);
                path.CloseFigure();
                using (var brush = new LinearGradientBrush(new PointF(0, 0), new PointF(n, n), Color.Black, Color.Black))
                {
                    brush.InterpolationColors = new ColorBlend
                    {
                        Colors = new[] { Color.FromArgb(0x4f, 0x5b, 0xf5), Color.FromArgb(0x9b, 0x51, 0xf0), Color.FromArgb(0xf0, 0x4a, 0x9a) },
                        Positions = new[] { 0f, 0.5f, 1f }
                    };
                    g.FillPath(brush, path);
                }
                // лёгкий блик сверху
                using (var shine = new LinearGradientBrush(new PointF(0, 0), new PointF(0, n * 0.6f),
                           Color.FromArgb(46, 255, 255, 255), Color.FromArgb(0, 255, 255, 255)))
                {
                    g.SetClip(path);
                    // чуть короче кисти, иначе на её границе видна полоска
                    g.FillRectangle(shine, 0, 0, n, n * 0.6f - 2);
                    g.ResetClip();
                }
            }

            float c = n / 2f;
            // кольцо компаса; на мелких размерах оно толще, чтобы не пропадало
            float ringR = n * 0.30f, ringW = Math.Max(n * 0.062f, 1.6f);
            using (var pen = new Pen(Color.White, ringW))
                g.DrawEllipse(pen, c - ringR, c - ringR, 2 * ringR, 2 * ringR);

            // стрелка: светлая половина смотрит на северо-восток
            float tip = n * 0.165f, waist = n * 0.062f;
            var ne = new PointF(c + tip, c - tip);
            var sw = new PointF(c - tip, c + tip);
            var nw = new PointF(c - waist, c - waist);
            var se = new PointF(c + waist, c + waist);
            using (var white = new SolidBrush(Color.White))
                g.FillPolygon(white, new[] { ne, nw, se });
            using (var faded = new SolidBrush(Color.FromArgb(150, 255, 255, 255)))
                g.FillPolygon(faded, new[] { sw, nw, se });
        }
        return bmp;
    }

    static byte[] Png(Bitmap bmp)
    {
        using (var ms = new MemoryStream()) { bmp.Save(ms, ImageFormat.Png); return ms.ToArray(); }
    }

    // Классическая запись значка: заголовок DIB, пиксели BGRA снизу вверх, пустая маска.
    static byte[] Dib(Bitmap bmp)
    {
        int n = bmp.Width, maskRow = ((n + 31) / 32) * 4;
        using (var ms = new MemoryStream())
        using (var w = new BinaryWriter(ms))
        {
            w.Write(40); w.Write(n); w.Write(n * 2); w.Write((short)1); w.Write((short)32);
            w.Write(0); w.Write(n * n * 4 + maskRow * n); w.Write(0); w.Write(0); w.Write(0); w.Write(0);
            for (int y = n - 1; y >= 0; y--)
                for (int x = 0; x < n; x++)
                {
                    Color p = bmp.GetPixel(x, y);
                    w.Write(p.B); w.Write(p.G); w.Write(p.R); w.Write(p.A);
                }
            w.Write(new byte[maskRow * n]);
            return ms.ToArray();
        }
    }

    public static void Save(string icoPath, string pngPath, int[] sizes)
    {
        var images = new List<byte[]>();
        foreach (int n in sizes)
            using (var bmp = Draw(n))
                images.Add(n >= 256 ? Png(bmp) : Dib(bmp));

        using (var fs = File.Create(icoPath))
        using (var w = new BinaryWriter(fs))
        {
            w.Write((short)0); w.Write((short)1); w.Write((short)sizes.Length);
            int offset = 6 + 16 * sizes.Length;
            for (int i = 0; i < sizes.Length; i++)
            {
                byte dim = (byte)(sizes[i] >= 256 ? 0 : sizes[i]);
                w.Write(dim); w.Write(dim); w.Write((byte)0); w.Write((byte)0);
                w.Write((short)1); w.Write((short)32);
                w.Write(images[i].Length); w.Write(offset);
                offset += images[i].Length;
            }
            foreach (var image in images) w.Write(image);
        }
        using (var big = Draw(512)) big.Save(pngPath, ImageFormat.Png);
    }
}
'@

$dir = Split-Path -Parent $MyInvocation.MyCommand.Path
[IconMaker]::Save("$dir\icon.ico", "$dir\icon.png", @(16, 20, 24, 32, 40, 48, 64, 128, 256))
"saved $dir\icon.ico ($((Get-Item "$dir\icon.ico").Length) bytes)"
