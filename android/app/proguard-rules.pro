# Мост для страниц (addJavascriptInterface) вызывается по имени — не переименовывать.
-keepclassmembers class dev.rustbrowser.PageBridge {
    @android.webkit.JavascriptInterface <methods>;
}
