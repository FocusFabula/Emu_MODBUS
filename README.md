# 🎓 Modbus Learning Tool: Symulator Slave w Rust

Projekt realizowany w ramach pracy inżynierskiej pt. **"Oprogramowanie do symulacji urządzenia slave protokołu MODBUS i wizualizacji danych"**. Program został zaprojektowany przede wszystkim jako **narzędzie edukacyjne**, ułatwiające zrozumienie niskopoziomowej komunikacji w automatyce przemysłowej.

## 🎯 Cel projektu
Głównym założeniem jest stworzenie przejrzystego środowiska do nauki protokołu Modbus. Dzięki implementacji w języku **Rust**, projekt pokazuje nowoczesne i bezpieczne podejście do programowania systemów, które tradycyjnie opierały się na języku C.

Program pozwala użytkownikowi "zajrzeć pod maskę" komunikacji przemysłowej, analizując jak dane są przechowywane, adresowane i przesyłane w formie binarnej.

## 🧩 Co ten projekt pomaga zrozumieć?

### 1. Architektura Pamięci (Moduł `device`)
Program symuluje fizyczną mapę pamięci sterownika. Pozwala to na praktyczną naukę różnic między czterema podstawowymi obszarami danych:
* **Coils (Cewki):** Odczyt i zapis pojedynczych bitów (np. sterowanie przekaźnikiem).
* **Discrete Inputs (Wejścia dyskretne):** Tylko odczyt stanów bitowych (np. czujnik zbliżeniowy).
* **Holding Registers (Rejestry rejestrujące):** Odczyt i zapis danych 16-bitowych (np. nastawa temperatury).
* **Input Registers (Rejestry wejściowe):** Tylko odczyt danych 16-bitowych (np. pomiar z przetwornika).



### 2. Przetwarzanie protokołu (Moduł `request` & `modbus_handle`)
Edukacyjna implementacja parsowania ramek PDU (Protocol Data Unit) pokazuje:
* Jak surowe bajty są interpretowane jako kody funkcyjne (np. kod `01` dla odczytu cewek).
* W jaki sposób system waliduje zapytania i generuje **wyjątki Modbus** (np. `IllegalDataAddress`).

## ⚙️ Kluczowe funkcjonalności
* **Pełna symulacja Slave:** Możliwość dowolnej konfiguracji liczby rejestrów i cewek.
* **Obsługa błędów:** Czytelne komunikaty o błędach adresowania i nieprawidłowych wartościach.
* **Modularność:** Kod podzielony na logiczne sekcje (pamięć, żądania, odpowiedzi), co ułatwia analizę poszczególnych etapów komunikacji.

---
*Projekt stanowi bazę dydaktyczną dla osób chcących zgłębić tajniki systemów SCADA, PLC oraz bezpiecznego programowania systemów wbudowanych.*
