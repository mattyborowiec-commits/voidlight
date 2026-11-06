# Voidlight: launcher Minecrafta (wczesna wersja)

Szybki launcher zbudowany w Tauri (Rust + lekki interfejs webowy).

## Uruchomienie (Windows)

1. Zainstaluj Rust (https://rustup.rs) i Node.js. Tauri wymaga też Microsoft C++ Build Tools i WebView2.
2. W tym folderze: `npm install`
3. Ikony (jednorazowo): `npm run icon -- sciezka\do\kwadratowego-obrazka.png`
4. Start: `npm run dev` (pierwsza kompilacja trwa kilka minut)

## Co działa

- Tworzenie instancji (Vanilla lub Fabric), wybór wersji gry, profilu JVM i RAM.
- Przycisk Graj: pobiera brakujące pliki (gra, biblioteki, zasoby, Java od Mojang), instaluje Fabric i uruchamia grę.
- Wszystko trafia do wspólnego folderu, więc kolejne starty niczego nie pobierają ponownie.
- Logowanie: na razie tylko nick offline (Ustawienia, Konto).
- Strona Mody (instancje Fabric): wyszukiwarka Modrinth, instalacja z automatycznymi zależnościami, pakiet wydajnościowy jednym kliknięciem (Sodium, Lithium, FerriteCore, Entity Culling, ModernFix, ImmediateFast), lista zainstalowanych z usuwaniem i aktualizacją wszystkich naraz. Modyfikacje są zablokowane, gdy gra działa.

## Gdzie są dane

`%APPDATA%\pl.voidlight.launcher\`
- `instances\<nazwa>\`: mody (`mods`), światy, `launcher.log` (wyjście Javy), `logs\latest.log` (log gry)
- `shared\`: wspólne biblioteki, zasoby, wersje i Java

## Ograniczenia tego etapu

- Wersje gry starsze niż około 1.19 mogą nie działać (stary format natywnych bibliotek).
- Quilt i NeoForge nie są jeszcze obsługiwane.
- Mody wrzucone ręcznie do folderu `mods` działają w grze, ale nie pojawiają się na liście ani w aktualizacjach (launcher zna tylko te, które sam zainstalował).
- Mod bez wersji dla Twojej wersji gry jest pomijany, a launcher pokazuje to w komunikacie (dotyczy to zwłaszcza bardzo nowych wersji Minecrafta).
- Pierwszy start pobiera orientacyjnie pół giga danych.

## Szybszy start gry

- Zasoby (dźwięki, języki) są weryfikowane raz; potem start pomija tysiące sprawdzeń plików.
- Java 19+ używa archiwum CDS (`instances/<nazwa>/cds/game.jsa`): pierwszy start je tworzy przy zamknięciu gry, kolejne ładują klasy szybciej.
- Sterta od razu docelowego rozmiaru (`-Xms` = `-Xmx`), wyłączone `UsePerfData`.
- Czas przygotowania przed startem jest w powiadomieniu i w `launcher.log`.
- Największy wpływ mają też: mod ModernFix (w paczce wydajnościowej), mniej modów i paczek zasobów, dysk SSD oraz opcjonalne wykluczenie folderu w Defenderze.

## Shadery, paczki zasobów i paczki danych

Na stronie Mody są przełączniki: Mody, Shadery, Paczki zasobów, Paczki danych (Modrinth). Shadery wymagają moda Iris (przycisk „Zainstaluj Iris”). Paczki danych instalują się w wybranym świecie (`saves/<świat>/datapacks`), więc świat musi już istnieć.

## Logowanie kontem Microsoft

1. W portalu Azure utwórz aplikację (App registrations → New registration, tylko osobiste konta Microsoft), włącz „Allow public client flows”.
2. Wklej „Application (client) ID” w Ustawienia → Konto → Konfiguracja logowania.
3. Mojang musi zatwierdzić ten Client ID (https://aka.ms/mce-reviewappid). Do tego czasu logowanie kończy się błędem „Invalid app registration”, a gra startuje w trybie offline.
4. Kliknij „Zaloguj przez Microsoft” i wpisz kod na stronie, którą otworzy launcher.

Token odświeżania leży w `%APPDATA%\pl.voidlight.launcher\auth\account.json` (bez szyfrowania).

## Import własnych plików

Na stronie Mody przeciągnij pliki na okno launchera: `.jar` dla modów, `.zip` dla shaderów, paczek zasobów i paczek danych (dla tych ostatnich wybierz najpierw świat). Pliki włożone ręcznie do folderu (przycisk „Otwórz folder”) też pojawiają się na liście jako „własny plik”. Własnych plików nie obejmuje „Aktualizuj wszystko”.

## Instalator

`npm run build` tworzy instalator NSIS (po polsku, instalacja bez uprawnień administratora) w `src-tauri\target\release\bundle\nsis\`. Ikony są już w `src-tauri\icons\`; `npm run icon -- obraz.png` wygeneruje je z innego obrazka.

## Włączanie, wyłączanie i aktualizacje

Na liście zainstalowanych pozycji każdy mod, shader i paczka ma przełącznik. Wyłączony plik dostaje dopisek `.disabled` (gra go pomija), a po ponownym włączeniu wraca do normalnej nazwy. Launcher sprawdza w tle, które pozycje mają nowszą wersję, oznacza je i pozwala zaktualizować pojedynczo lub wszystkie naraz.

## Strona instancji

Dwuklik na karcie instancji otwiera jej stronę: nagłówek z przyciskiem Graj, zakładka Zawartość (mody, shadery, paczki zasobów i danych z przełącznikami) oraz Ustawienia, gdzie możesz zmienić nazwę, pamięć RAM i profil, zrobić kopię (opcjonalnie ze światami) albo usunąć instancję. Wersji gry i loadera w istniejącej instancji nie zmieniamy, żeby nie zepsuć światów i modów.

## Import modpacków (.mrpack)
Przeciągnij plik `.mrpack` (z Modrinth) na okno launchera na dowolnej stronie. Powstanie nowa instancja z właściwą wersją Minecrafta i Fabric Loaderem, a pliki paczki zostaną pobrane (z weryfikacją SHA-1) i wypakowane razem z `overrides/` oraz `client-overrides/`. Pomijane są pliki tylko serwerowe; opcjonalne są instalowane. Paczki na Forge/NeoForge/Quilt są odrzucane z komunikatem. Pobieranie jest dozwolone tylko z cdn.modrinth.com, github.com, raw.githubusercontent.com i gitlab.com, a ścieżki z `..` lub bezwzględne są blokowane. Zaimportowane mody pojawiają się na liście jako pliki lokalne (bez znaczników aktualizacji).

## Budowanie bez administratora
Dwukliknij `ZBUDUJ.bat`. Skrypt pobierze przenosne narzedzia do `.toolchain`, zbuduje instalator, zapisze go jako `Voidlight-instalator.exe` (ten plik mozesz dac kolegom) i uruchomi. Log: `.toolchain\build.log`. Pierwszy raz trwa dluzej.

## Modpacki z Modrinth
Na stronie Instancje przycisk „Modpacki z Modrinth” otwiera wyszukiwarkę modpacków (Fabric). Skrót „Create” wyszukuje paczki z modem Create. „Zainstaluj” pobiera całą paczkę ze wszystkimi modami i tworzy z niej nową instancję (jak import `.mrpack`).
