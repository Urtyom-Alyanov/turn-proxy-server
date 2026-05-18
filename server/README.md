# TURN Proxy (Сервер)

Слушает порт (здесь по умолчанию 56040), на который идёт DTLS трафик, полученный от клиента напрямую или через TURN
сервер. Далее полученный трафик он терминирует и получает UDP, который пересылает на определённый порт (например 51820,
если вы используете WireGuard).

В коде это изображено как оборачивание UDP сокета на 56040 порту в некое DTLS соединение.

## Развёртка

### Windows

Для ОС Windows можно скачать во вкладке ["релизы"](https://github.com/Urtyom-Alyanov/turn-proxy/releases/latest).

### Linux-based

На данный момент доступны [`flake.nix`](./flake.nix) для пакетного менеджера Nix вместе с модулем для NixOS, а также
[`PKGBUILD`](./PKGBUILD) для Arch Linux

#### Быстрая установка (Debian/Ubuntu/Fedora/и производные)

```bash
curl -sSL https://raw.githubusercontent.com/Urtyom-Alyanov/turn-proxy-server/master/install-server.sh | bash
```

##### Минимальные требования

- Ubuntu 21.10 (или 22.04 LTS)
- Debian 12 (Bookworm)
- Fedora 35
- RHEL / CentOS 9

Проще говоря, нужна минимальная версия glibc: `2.27`

#### NixOS

Для наилучшей операционной системы модно, можно и надо использовать модули, а если быть точнее, то модуль который
можно импортировать с помощью [`flake.nix`](../flake.nix). Также его декларация находится здесь: [`nix/module.nix`](./nix/module.nix)

В вашем flake.nix просто укажите:

```nix
{
  inputs = {
    turn-proxy.url = "github:Urtyom-Alyanov/turn-proxy";
  };
  outputs = { turn-proxy }: {
    # импортируйте куда нибудь
    # модуль turn-proxy.nixosModules.server
    # или же пакет turn-proxy.packages.${pkgs.system}.server
  };
}
```

#### Для Arch Linux (PKGBUILD)

```shell
# Когда опубликуется на AUR
#git clone [https://aur.archlinux.org/turn-proxy-server-rs.git](https://aur.archlinux.org/turn-proxy-server-rs.git)
#cd turn-proxy-server-rs
#makepkg -si

# Поэтому пока так
git clone [https://github.com/Urtyom-Alyanov/turn-proxy-server.git](https://aur.archlinux.org/turn-proxy-server.git)
cd turn-proxy-server
makepkg -si
```

#### Dockerfile

Также есть [Dockerfile](./Dockerfile), но сам пакет не опубликован на Dockerhub, делайте с ним что хотите.

##### Проблема костыльности

##### Пример `docker-compose.yaml`
```yaml
networks:
  internal_network:
    driver: bridge

services:
  turn-proxy:
    image: ghcr.io/Urtyom-Alyanov/turn-proxy:latest
    container_name: turn-proxy
    restart: always
    volumes:
      - /srv/turn-proxy/config.toml:/config.toml:ro
    ports:
       - 56040:56040/udp
    #networks:
      - internal_network

  wireguard:
    image: lscr.io/linuxserver/wireguard:latest
    container_name: wireguard
    restart: always
    networks:
      - internal_network
    cap_add:
      - NET_ADMIN
      - SYS_MODULE
    environment:
      - PUID=1000
      - PGID=1000
      - TZ=Europe/Moscow
    sysctls:
      - net.ipv4.ip_forward=1
    volumes:
      - /srv/wireguard:/config:Z
      - /lib/modules:/lib/modules:ro
```

Мы будем учитывать, что конфигурация (и файлы, с связанные с ним) для wireguard располагается на пути `/srv/wireguard`
Вот пример файла, находящегося на пути `/srv/wireguard/wg_confs/wg0.conf`
```conf
[Interface]
Address = 10.0.0.1/24
ListenPort = 51820
PrivateKey = <приватный ключ сервера>

PostUp = INTERFACE=$(ip route show default | awk '{print $5}'); iptables -A FORWARD -i %i -j ACCEPT; iptables -A FORWARD -o %i -j ACCEPT; iptables -t nat -A POSTROUTING -o $INTERFACE -j MASQUERADE
PostDown = INTERFACE=$(ip route show default | awk '{print $5}'); iptables -D FORWARD -i %i -j ACCEPT; iptables -D FORWARD -o %i -j ACCEPT; iptables -t nat -D POSTROUTING -o $INTERFACE -j MASQUERADE

[Peer]
PublicKey = <публичный ключ клиента>
AllowedIPs = 10.0.0.2/32
```

А теперь пример конфигурации, находящегося в файле по пути `/srv/turn-proxy/config.toml`

```toml
[common]
listening_on = "0.0.0.0:56040"
proxy_into = "wireguard:51820" # Если что, внутри докера (или подмана) wireguard это ссылка на контейнер, где лежит сам wireguard, это нужно, чтобы не святить им в открытый интернет
max_connections = 2000
```

## Использование

По умолчанию программа ищет конфигурацию в `/etc/turn-proxy/server/config.toml`, однако можно задать и иной путь
с помощью `--config {путь}`.

### Конфигурация

Конфигурационный файл имеет следующую структуру:

```toml
[common]
listening_on = "0.0.0.0:56000" # Адрес, который слушает программа, то есть куда будет обращаться TURN сервер с зашифрованным (с помощью DTLS) трафиком (адресант)
proxy_into = "127.0.0.1:51820" # Адрес, куда будет высылаться расшифрованный UDP-трафик (адресат)
max_connections = 2000 # Максимальное число соединений
```

Если вы используете NixOS, то можно использовать модуль, пример ниже

```nix
{
  services.turn-proxy.server = {
    enable = true; # Включаем шарманку
    config = {
      listeningOn = "0.0.0.0:56000"; # Адрес, который слушает программа, то есть куда будет обращаться TURN сервер с зашифрованным (с помощью DTLS) трафиком (адресант)
      proxyInto = "127.0.0.1:51820"; # Адрес, куда будет высылаться расшифрованный UDP-трафик (адресат)
      maxConnections = 2000;
    };
    configFile = ./config.toml; # Также никто не мешает указать просто файл с конфигурацией
    # Также есть ещё аргумент package, чтобы задать кастомный бинарник
  };
}
```

Также можно указать **в качестве аргументов**, если вы собираетесь запускать деобфускатор вручную:

```shell
turn-proxy-server --no-config --listening-on=0.0.0.0:56000 --proxy-into=127.0.0.1:51820
```

### Сервисы

Однако рекомендуется запускать его с помощью сервиса, для systemd сервис называется `turn-proxy-server.service`,
манипуляции с ним такие же, как и с другими сервисами, учтите, что последние 2 команды не применимы к NixOS из-за её
природы

```shell
systemctl start turn-proxy-server.service # Чтобы его запустить
systemctl stop turn-proxy-server.service # Чтобы его остановить
systemctl restart turn-proxy-server.service # Чтобы его перезапустить (если он залагал или конфигурацию поменяли)
systemctl enable --now turn-proxy-server.service # Чтобы его запускать вместе с системой (флаг --now его запустит в ту же секунду)
systemctl disable --now turn-proxy-server.service # Чтобы его НЕ запускать вместе с системой (флаг --now его остановит в ту же секунду)
```

В качестве адресата можно использовать любые протоколы, работающие поверх UDP, например WireGuard или Hysteria2.

**ПРИ ИСПОЛЬЗОВАНИИ WIREGUARD рекомендуется поставить MTU равным 1280-1380, так как TURN и DTLS добавляют свои заголовки,
что может привести к фрагментации пакетов, так как пакеты могут уже не влезать в стандартное ограничение в 1500 байт,
что приведёт к резкому снижению скорости, а она и так невелика.**

---

_Сервер лицензирован под лицензией [AGPLv3](./LICENSE)_
