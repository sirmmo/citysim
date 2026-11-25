// citysim's own JavaScript: local storage for saves, the page's query string
// (and an OpenTactics #holding= link) as the command line, exports as
// downloads, HTTP requests to region servers and the clipboard. The Rust side is crates/opencity/src/web.rs; the version must
// match citysim_crate_version().
// The browser's offer to install the page as an app, kept until the player
// picks File > Install as an app (Chrome and Edge make the offer; Firefox
// and Safari install from their own menus and never fire this).
var citysim_install_offer = null;
window.addEventListener("beforeinstallprompt", function (e) {
  e.preventDefault();
  citysim_install_offer = e;
});
window.addEventListener("appinstalled", function () { citysim_install_offer = null; });
// A newer OpenTactics link opened in this tab: start again with it.
window.addEventListener("hashchange", function () {
  if (window.location.hash.indexOf("#holding") === 0) { window.location.reload(); }
});

function citysim_status(text) {
  var el = document.getElementById("status-text");
  if (el) { el.textContent = text; }
  var box = document.getElementById("status");
  if (box) { box.classList.remove("hidden"); }
}

miniquad_add_plugin({
  name: "citysim",
  version: 4,
  register_plugin: function (importObject) {
    importObject.env.citysim_storage_get = function (key) {
      key = consume_js_object(key);
      try {
        var v = window.localStorage.getItem(key);
        return js_object(v === null ? undefined : v);
      } catch (e) {
        console.warn("localStorage:", e);
        return js_object(undefined);
      }
    };
    importObject.env.citysim_storage_set = function (key, value) {
      key = consume_js_object(key);
      value = consume_js_object(value);
      try {
        window.localStorage.setItem(key, value);
        return 1;
      } catch (e) {
        console.warn("localStorage:", e);
        return 0;
      }
    };
    // quad-url (a macroquad dependency) is not bundled: its one import here.
    importObject.env.quad_url_link_open = function (ptr, len) {
      window.open(UTF8ToString(ptr, len), "_blank");
    };
    importObject.env.citysim_storage_keys = function () {
      var keys = [];
      try {
        for (var i = 0; i < window.localStorage.length; i++) { keys.push(window.localStorage.key(i)); }
      } catch (e) {
        console.warn("localStorage:", e);
      }
      return js_object(JSON.stringify(keys));
    };
    // HTTP requests (region servers): started here, answered when polled.
    // The result is `{"status": 200, "body": "..."}`; status 0 is a network
    // error (the server is down, blocks the origin or timed out) with the
    // reason as the body.
    var requests = { next: 1, done: {} };
    importObject.env.citysim_http_start = function (method, url, body) {
      method = consume_js_object(method);
      url = consume_js_object(url);
      body = consume_js_object(body);
      var id = requests.next++;
      var options = { method: method, headers: {}, cache: "no-store" };
      if (body.length > 0) {
        options.body = body;
        options.headers["Content-Type"] = "application/json";
      }
      var finish = function (status, text) {
        requests.done[id] = JSON.stringify({ status: status, body: text });
      };
      try {
        var controller = typeof AbortController !== "undefined" ? new AbortController() : null;
        if (controller) {
          options.signal = controller.signal;
          setTimeout(function () { controller.abort(); }, 8000);
        }
        fetch(url, options).then(function (r) {
          return r.text().then(function (t) { finish(r.status, t); });
        }).catch(function (e) {
          finish(0, e && e.name === "AbortError" ? "timed out after 8 s" : String(e));
        });
      } catch (e) {
        finish(0, String(e));
      }
      return id;
    };
    importObject.env.citysim_http_poll = function (id) {
      var r = requests.done[id];
      if (r === undefined) { return js_object(undefined); }
      delete requests.done[id];
      return js_object(r);
    };
    importObject.env.citysim_can_install = function () {
      return citysim_install_offer ? 1 : 0;
    };
    importObject.env.citysim_install = function () {
      var offer = citysim_install_offer;
      if (!offer) { return; }
      // An offer can be shown once; the browser makes a new one if the
      // player says no.
      citysim_install_offer = null;
      try { offer.prompt(); } catch (e) { console.warn("install:", e); }
    };
    importObject.env.citysim_query = function () {
      return js_object(window.location.search || "");
    };
    importObject.env.citysim_hash = function () {
      return js_object(window.location.hash || "");
    };
    importObject.env.citysim_copy = function (text) {
      text = consume_js_object(text);
      var fallback = function () {
        var area = document.createElement("textarea");
        area.value = text;
        document.body.appendChild(area);
        area.select();
        try { document.execCommand("copy"); } catch (e) { console.warn("copy:", e); }
        area.remove();
      };
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text).catch(fallback);
      } else {
        fallback();
      }
    };
    importObject.env.citysim_open = function (url) {
      window.open(consume_js_object(url), "_blank");
    };
    importObject.env.citysim_download = function (name, text) {
      name = consume_js_object(name);
      text = consume_js_object(text);
      var blob = new Blob([text], { type: "application/octet-stream" });
      var a = document.createElement("a");
      a.href = URL.createObjectURL(blob);
      a.download = name;
      document.body.appendChild(a);
      a.click();
      setTimeout(function () { URL.revokeObjectURL(a.href); a.remove(); }, 1000);
    };
  },
  on_init: function () {
    // The game draws its own loading screen from here on (on Wavedash, show
    // it: wavedash.js).
    var box = document.getElementById("status");
    if (box) { box.classList.add("hidden"); }
    if (window.WavedashHook) { window.WavedashHook.ready(); }
  }
});
