

<div align="center">

<img src="assets/icons/app/io.github.AghastyGD.Wiretray.png" width="180">

# Wiretray

**Un administrador de puntos de acceso Wi-Fi para Linux.**

![CI](https://github.com/aghastygd/wiretray/actions/workflows/ci.yml/badge.svg)
![License](https://img.shields.io/github/license/aghastygd/wiretray)
![Issues](https://img.shields.io/github/issues/aghastygd/wiretray)
</div>

Wiretray es un administrador de puntos de acceso Wi-Fi para Linux diseñado para hacer que el control del punto de acceso esté disponible directamente desde la bandeja del sistema.

Construido alrededor de NetworkManager, ofrece gestión, configuración e inicio automático del punto de acceso a través de una interfaz de escritorio.

## Motivación

Este proyecto se inspiró en [Linux WiFi Hotspot](https://github.com/lakinduakash/linux-wifi-hotspot).

Lo utilicé durante un tiempo y tuve una buena experiencia con él, pero una cosa siempre me molestaba: cada vez que quería habilitar o deshabilitar el punto de acceso, primero tenía que abrir la ventana de la aplicación.

Buscaba una solución que pudiera permanecer en la bandeja del sistema y permitir gestionar el punto de acceso con solo unos clics.

Wiretray comenzó como un experimento, pero rápidamente evolucionó hasta convertirse en una herramienta que uso a diario para gestionar puntos de acceso en Linux.

El objetivo a largo plazo es simple: que la gestión del punto de acceso se sienta como cualquier otro servicio en segundo plano en Linux.

## Características

Disponibles actualmente:

- [x] Integración en la bandeja del sistema
    
- [x] Interfaz de configuración de escritorio
    
- [x] Detección de dispositivos Wi-Fi
    
- [x] Creación y gestión de puntos de acceso
    
- [x] Monitoreo del estado del punto de acceso
    
- [x] Persistencia de la configuración del punto de acceso
    
- [x] Inicio automático al iniciar sesión
    
- [x] Detección de capacidades Wi-Fi
    

En desarrollo actualmente:

-   Mejor informe de errores
-   Soporte concurrente para AP + Cliente

## Requisitos

-   Linux
-   NetworkManager
-   Un adaptador Wi-Fi con soporte para Punto de Acceso (AP)
-   Un entorno de escritorio o panel con soporte para StatusNotifierItem/AppIndicator

## Instalación

### Debian / Ubuntu

Descarga el paquete `.deb` más reciente desde la página de [Releases](https://github.com/AghastyGD/wiretray/releases) e instálalo con:

```
sudo apt install ./wiretray_*.deb
```

También puedes instalarlo haciendo doble clic en el paquete `.deb` desde tu administrador de archivos.

Después de la instalación, Wiretray se puede abrir desde el menú de aplicaciones.

El lanzador de la aplicación abre la ventana de configuración e inicia el proceso de la bandeja automáticamente si aún no se está ejecutando.

## Inicio automático

Wiretray puede iniciarse automáticamente después de iniciar sesión.

Abre la ventana de configuración y habilita:

```
Iniciar automáticamente al iniciar sesión
```

Esto crea una entrada de inicio automático XDG en:

```
~/.config/autostart/io.github.AghastyGD.Wiretray.desktop
```

Cuando está habilitado, Wiretray inicia el proceso de la bandeja/en segundo plano automáticamente la próxima vez que inicies sesión.

## Binarios instalados

El paquete Debian instala dos binarios:

```
wiretray
wiretray-settings
```

`wiretray` inicia la aplicación de la bandeja/en segundo plano.

`wiretray-settings` abre la interfaz de configuración de escritorio.

Normalmente, los usuarios no necesitan ejecutarlos manualmente. El lanzador de la aplicación y la entrada de inicio automático se encargan de esto automáticamente.

## Compilación desde el código fuente

Instala los paquetes de desarrollo requeridos para tu distribución.

### Debian / Ubuntu

```
sudo apt install \
  libgtk-4-dev \
  meson \
  desktop-file-utils \
  gcc \
  gtk-update-icon-cache
```

### Fedora

```
sudo dnf install \
  gtk4-devel \
  meson \
  desktop-file-utils \
  gcc \
  glib2-devel \
  gtk4-update-icon-cache
```

### Arch Linux

```
sudo pacman -S \
  gtk4 \
  meson \
  desktop-file-utils \
  gcc
```

Compilar todos los binarios:

```
cargo build
```

O compilar un binario específico:

```
cargo build --bin wiretray
cargo build --bin wiretray-settings
```

## Ejecución desde el código fuente

Iniciar la aplicación de la bandeja:

```
cargo run --bin wiretray
```

Iniciar la aplicación de configuración directamente:

```
cargo run --bin wiretray-settings
```

## Desarrollo

Formatear el código:

```
cargo fmt
```

Ejecutar Clippy:

```
cargo clippy --all-targets --all-features -- -D warnings
```

Ejecutar pruebas:

```
cargo test
```

## Hoja de ruta

El trabajo planificado incluye:

-   Generación de códigos QR
-   Monitoreo de clientes conectados
-   Notificaciones del punto de acceso
-   Configuración avanzada del punto de acceso
-   Backends alternativos para puntos de acceso

Como ocurre con la mayoría de los proyectos personales, las prioridades pueden cambiar con el tiempo.

## Contribuciones

Son bienvenidas las incidencias, sugerencias y pull requests.

## Licencia

Este proyecto está licenciado bajo la Licencia MIT.
