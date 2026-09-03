{ pkgs ? import <nixpkgs> {} }:
let
  androidComposition = pkgs.androidenv.composeAndroidPackages {
    platformVersions = [ "34" "35" ];
    buildToolsVersions = [ "34.0.0" "35.0.0" ];

    includeNDK = true;
    ndkVersions = [ "29.0.14206865" ];

    includeEmulator = true;

    includeSystemImages = true;
    systemImageTypes = [ "google_apis" ];
    abiVersions = [ "x86_64" ];
  };

  androidSdk = androidComposition.androidsdk;
in
pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    pkg-config
    gnumake
    gcc
    perl
    tailwindcss
    jdk17
    androidSdk
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
  JAVA_HOME = "${pkgs.jdk17}";
  NIXPKGS_ACCEPT_ANDROID_SDK_LICENSE = "1";

  ANDROID_HOME = "${androidSdk}/libexec/android-sdk";
  ANDROID_SDK_ROOT = "${androidSdk}/libexec/android-sdk";

  ANDROID_NDK_HOME =
    "${androidSdk}/libexec/android-sdk/ndk/29.0.14206865";

  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [
    xdotool
    gtk3
    webkitgtk_4_1
    glib
  ]);
}
