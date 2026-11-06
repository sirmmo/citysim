// citysim's own JavaScript: local storage for saves, the page's query string
// as the command line and exports as downloads. The Rust side is
// crates/opencity/src/web.rs; the version must match citysim_crate_version().
function citysim_status(text) {
  var el = document.getElementById("status-text");
  if (el) { el.textContent = text; }
  var box = document.getElementById("status");
  if (box) { box.classList.remove("hidden"); }
}

miniquad_add_plugin({
  name: "citysim",
  version: 1,
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
    importObject.env.citysim_query = function () {
      return js_object(window.location.search || "");
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
    // The game draws its own loading screen from here on.
    var box = document.getElementById("status");
    if (box) { box.classList.add("hidden"); }
  }
});
