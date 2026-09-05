{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    pkg-config
    gnumake
    gcc
    perl
    tailwindcss
  ];

  buildInputs = with pkgs; [
    glib
    gtk3
    webkitgtk_4_1
    libsoup_3
    cairo
    pango
    atk
    gdk-pixbuf
    openssl
    xdotool
    glib.dev
    gtk3.dev
    webkitgtk_4_1.dev
    libsoup_3.dev
    cairo.dev
    pango.dev
    atk.dev
    gdk-pixbuf.dev
    openssl.dev
  ];

  # Передаем путь к бинарным библиотекам (.so), чтобы линкер (ld) находил libxdo.so
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [
    xdotool
    gtk3
    webkitgtk_4_1
    glib
  ]);
}
