# Открытые стандарты и форматы обмена: справка для Open BIM

Дата сбора: 2026-10-20. Источники с датами доступа перечислены в [sources.md](sources.md).
Метка **«не подтверждено»** означает, что сведение не удалось проверить по
первоисточнику в этом сборе; рядом указано, что найдено.

Что Open BIM умеет сейчас (по `docs/ifc-support.md`, `docs/parity.csv`): экспорт и
импорт IFC4 ADD2 TC1 (STEP, мм). В подмножество входят `IfcBeam`, `IfcColumn`, `IfcPlate`,
`IfcExtrudedAreaSolid`, шесть параметрических профилей, `IfcArbitraryClosedProfileDef`,
`IfcOpeningElement`, пространственная структура, `IfcMaterial` с плотностью и pset
«Open BIM». Болты, швы, сборки, оси, узлы и чертежи при экспорте **пропускаются**.
IFC4X3 читается как IFC4 с предупреждением, IFC2X3 отклоняется. DXF Open BIM пишет.
DSTV, DWG, BCF, IDS не поддерживаются.

Сводная таблица приоритетов (подробности ниже):

| Стандарт / формат | Текущая версия | Open BIM сейчас | Приоритет |
|---|---|---|---|
| IFC4 ADD2 TC1 | 4.0.2.1 = ISO 16739-1:2018 (отменён ISO) | подмножество, без сборок и крепежа | **P1**: расширить до стального Reference View |
| IFC4.3 | 4.3.2.0 = ISO 16739-1:2024 | только чтение как IFC4 | P2 |
| BCF | 3.0 (2.1 широко распространена) | нет | **P1/P2** (вместе с reference-моделями) |
| IDS | 1.0 (июнь 2024) | нет | P2 |
| MVD Reference View / Design Transfer View | RV 1.2 для IFC4; DTV официально не выпущен | нет декларации MVD | **P1** (RV) / P3 (DTV) |
| bSDD | онлайн-сервис, API | нет | P3 |
| DSTV-NC | 7-е изд. (1998) — базовое; новее — не подтверждено | нет | **P1** |
| DSTV «Product interface» (DSTV-BV) | не подтверждено | нет | P3 |
| DXF / DWG | DXF пишется; DWG — только через LibreDWG/ODA | DXF да, DWG нет | DWG — P2/P3 |
| CIS/2 | наследие, развития нет | нет | P3 (не делать) |
| glTF 2.0 / XKT | ISO/IEC 12113:2022 / XKT (xeokit) | нет | P2 (glTF), P3 (XKT) |

---

## 1. IFC4 ADD2 TC1

- **Что это.** Industry Foundation Classes — открытая схема данных buildingSMART.
  IFC4 ADD2 TC1 (версия 4.0.2.1) соответствует ISO 16739-1:2018. В России она принята
  как **ГОСТ Р 10.0.02-2019/ИСО 16739-1:2018**, действует с 01.09.2019.
- **Статус.** На стороне ISO редакция 2018 года **отменена** и заменена ISO 16739-1:2024
  (IFC4.3). buildingSMART и российский ГОСТ Р 10.0.02-2019 продолжают на неё ссылаться.
  Российские экспертизы и Москва по-прежнему требуют именно IFC4 (4.0.2.1) или IFC2x3, см.
  [russian-regulation.md](russian-regulation.md).
- **Кто использует.** Все крупные системы. Коммерческие аналоги экспортируют IFC4 Reference view
  (часть ещё Design transfer view и Precast view), подгружают IFC4 как связь при импорте;
  отечественные системы экспортируют IFC4 с Reference View по умолчанию.
  IfcOpenShell полностью поддерживает геометрию IFC4 ADD2 TC1.
- **Что важно для стали** (сущности из схемы IFC4/IFC4.3):
  - элементы: `IfcBeam`, `IfcColumn`, `IfcMember` (связи, раскосы, пояса ферм), `IfcPlate`;
    обрезки и отверстия — `IfcOpeningElement` + `IfcRelVoidsElement` или
    `IfcBooleanClippingResult`;
  - сборки: `IfcElementAssembly` (PredefinedType, например TRUSS, GIRDER, BRACED_FRAME;
    перечень значений не перепроверен в этом сборе), состав — `IfcRelAggregates`;
  - крепёж: `IfcMechanicalFastener` / `IfcMechanicalFastenerType` (PredefinedType
    BOLT, ANCHORBOLT и др.; `NominalDiameter`, `NominalLength`), pset
    `Pset_MechanicalFastenerBolt` (ThreadDiameter, ThreadLength, NutsCount,
    WashersCount, HeadShape, KeyShape, NutShape, WasherShape),
    `Pset_MechanicalFastenerAnchorBolt`;
  - сварка: `IfcFastener` с pset `Pset_FastenerWeld`. Значение WELD в
    `IfcFastenerTypeEnum` в этом сборе не подтверждено (страница схемы перечень не показала);
  - связь крепежа с деталями: `IfcRelConnectsWithRealizingElements` или включение
    в `IfcElementAssembly`;
  - профили: `IfcIShapeProfileDef`, `IfcLShapeProfileDef`, `IfcUShapeProfileDef`,
    `IfcTShapeProfileDef`, `IfcCShapeProfileDef`, `IfcZShapeProfileDef`,
    `IfcRectangleHollowProfileDef`, `IfcCircleHollowProfileDef`, `IfcRectangleProfileDef`,
    `IfcCircleProfileDef`, `IfcArbitraryClosedProfileDef`; для сварных сечений —
    `IfcCompositeProfileDef`. Имя профиля пишется в `ProfileName`, например «20Б1» или
    «L100x8»;
  - материал по профилю: `IfcMaterialProfileSet` → `IfcMaterialProfile` (материал +
    профиль), привязка через `IfcRelAssociatesMaterial` к типу (`IfcBeamType`), а у
    экземпляра — `IfcMaterialProfileSetUsage` (CardinalPoint = точка привязки
    сечения). Это основной способ передать «профиль + марку стали» в Reference View;
  - типы: `IfcBeamType`, `IfcColumnType`, `IfcPlateType`, `IfcMemberType` + `IfcRelDefinesByType`;
  - свойства: `Pset_BeamCommon` (Reference, Status, Span, Slope, Roll, IsExternal,
    ThermalTransmittance, LoadBearing, FireRating), аналогично `Pset_ColumnCommon`,
    `Pset_MemberCommon`, `Pset_PlateCommon`; материал — `Pset_MaterialCommon`
    (MassDensity), `Pset_MaterialSteel` (марка, пределы текучести и прочности);
  - количества: `Qto_BeamBaseQuantities`, `Qto_ColumnBaseQuantities`,
    `Qto_MemberBaseQuantities`, `Qto_PlateBaseQuantities` (длина, площадь сечения,
    объём, масса). Состав полей Qto в этом сборе не перепроверялся;
  - оси: `IfcGrid` / `IfcGridAxis`; классификация: `IfcClassification` +
    `IfcClassificationReference` + `IfcRelAssociatesClassification` (так передаётся код КСИ);
  - геопривязка (IFC4): `IfcMapConversion` + `IfcProjectedCRS`.
- **Лицензия.** Схема и документация buildingSMART открыты для бесплатного использования.
  Текст ISO 16739-1 платный.
- **Библиотеки.**
  - IfcOpenShell (C++/Python, LGPL-3.0-or-later; 0.9.0.post1 от 15.10.2026, есть колёса
    для macOS arm64) — эталон для независимой проверки, Open BIM им уже пользуется в тестах;
  - Rust: `ifc-lite-core` (MPL-2.0, 5.0.0; парсер STEP и геометрия, IFC4/IFC4X3/IFC5);
    `ifc_rs` (MIT, 0.1.0-alpha; чтение и построители); `openbim-step` (openbimrs, лицензия
    не проверена);
  - web-ifc (That Open, C++→WASM, MPL-2.0).

  Собственный экспортёр Open BIM (`crates/ifc`, Apache-2.0) уже работает. Брать
  внешнюю зависимость нужно только для импорта сложной геометрии.
- **Приоритет для Open BIM: P1.** Добавить в экспорт `IfcElementAssembly`,
  `IfcMechanicalFastener`, сварные швы, `IfcGrid`, `IfcMaterialProfileSet(Usage)`,
  типы, стандартные `Pset_*Common` и `Qto_*`, а также `IfcClassificationReference` (КСИ).
  Без этого модель не проходит требования ЦГЭ: сборки → `IfcElementAssembly`, крепёж →
  `IfcMechanicalFastener`.

## 2. IFC4.3 (ISO 16739-1:2024)

- **Что это.** IFC 4.3.2.0 (IFC4X3_ADD2) — расширение IFC4 на инфраструктуру: дороги,
  ж/д, мосты, порты. ISO 16739-1:2024, издание 2, опубликован в марте 2024 (жизненный
  цикл ISO: 2024-03-22), 496 с., заменил ISO 16739-1:2018. buildingSMART объявил
  ISO-публикацию 02.04.2024.
- **Кто использует.** Ведущие коммерческие аналоги экспортируют IFC4.3 (Reference view,
  Design transfer view, Bridge view; домен Building). IfcOpenShell разбирает IFC4x3 Add2,
  но геометрию официально поддерживает только для IFC2x3 и IFC4. Часть коммерческих
  систем пока экспортирует только IFC4/IFC2x3/IFC2x2.
- **Сталь.** Для зданий набор стальных сущностей тот же, что в IFC4, плюс новые
  PredefinedType и pset. Для мостов добавлены `IfcBridge` и `IfcBridgePart`.
- **Российский статус.** Российского ГОСТ на IFC4.3 в этом сборе **не найдено** — не
  подтверждено. Экспертизы требуют IFC4 (4.0.2.1) или IFC2x3.
- **Библиотеки:** IfcOpenShell, ifc-lite-core.
- **Приоритет: P2** (открытая строка `ifc43` в `parity.csv`). Сначала полный IFC4 RV,
  затем переключатель схемы IFC4X3 в экспорте. Отличия в основном в заголовке и
  перечислениях.

## 3. BCF 3.0 (BIM Collaboration Format)

- **Что это.** Формат замечаний по модели: тема, комментарии, точки обзора с камерой,
  видимостью и выбранными объектами по IFC GlobalId, снимки. Бывает файловым (BCF-XML,
  zip-архив `.bcf`/`.bcfzip`) и серверным (BCF-API, REST).
- **Версии.** 3.0 — текущая версия buildingSMART. Точную дату выпуска 3.0 по первоисточнику
  установить не удалось — **не подтверждено**: страница buildingSMART описывает только
  вебинар о версии 3.0, а вики OSArch устарела и называет стабильной 2.1. На практике
  обмениваются в основном BCF 2.1. Запросы пользователей на поддержку 3.0 есть и у коммерческих систем.
- **Кто использует.** Коммерческие аналоги хранят темы BCF в облачной среде и экспортируют
  .bcf/.pdf, привязывая объекты по идентификаторам. Также BCF есть в Bonsai, IfcOpenShell
  (`bcf`, включая BCF-API) и That Open Engine (BCFTopics). Москва требует для АГР
  BCF-файл или контейнер с аннотациями.
- **Лицензия:** открытый стандарт buildingSMART (GitHub `buildingSMART/BCF-XML`, `BCF-API`).
- **Rust.** `openbim-bcf` 0.6.1 (openbimrs) читает и пишет 2.1 и 3.0, но лицензия
  **AGPL-3.0-or-later** несовместима с поставкой Apache-2.0 без смены лицензии всего
  приложения. Альтернатива — свой модуль на `quick-xml` + `zip` (MIT/Apache): формат
  небольшой, XSD открыты.
- **Приоритет: P1/P2.** Вместе с reference-моделями (строка `reference-models`) BCF даёт
  координацию со смежниками. Писать 2.1 и 3.0, читать оба.

## 4. IDS 1.0 (Information Delivery Specification)

- **Что это.** Машиночитаемые требования к модели в XML по XSD: какие классы IFC,
  свойства, классификация, материалы и значения обязательны. Модель проверяется
  автоматически. Проверяемые аспекты называются фасетами: entity, attribute, classification,
  property, material, partOf. Перечень фасетов дан по общим сведениям; страница
  buildingSMART называет объекты, классификации, материалы, свойства и значения.
- **Версия.** 1.0 утверждена как финальный стандарт buildingSMART: 1 июня 2024 по
  странице стандарта, объявление от 4 июня 2024. Версии 1.1 и 2.0 в разработке.
- **Кто использует.** IfcOpenShell `ifctester` (библиотека, CLI, веб); Solibri, usBIM.IDS,
  That Open (IDSSpecifications). Санкт-Петербургская ЦГЭ публикует требования к ЦИМ
  в виде IDS-файла (по статье на Хабре).
- **Rust:** `openbim-ids` 0.2.2 (MIT; функции аудита тянут AGPL-зависимости). Проверку
  IFC-модели по IDS крейт **не реализует**.
- **Приоритет: P2.** Встроенная проверка «модель соответствует IDS заказчика или
  экспертизы» перед выпуском. Первый шаг — независимая проверка через `ifctester` в тестах.

## 5. MVD: Reference View и Design Transfer View

- **Reference View (RV)** — подмножество IFC4 для координации и передачи «только для
  просмотра и справки»: геометрия в основном tessellation/extrusion, без параметрики.
  Последняя версия для IFC4 — **RV 1.2** (документация на standards.buildingsmart.org,
  ADD2_TC1/RV1_2). Это самая зрелая MVD для IFC4. Тот же вид требуют в Москве
  («IFC 4 Reference View») и рекомендуют в Краснодарской экспертизе.
- **Design Transfer View (DTV)** — передача с возможностью редактирования
  (параметрическая геометрия). Политика buildingSMART по MVD (апрель 2021) прямо
  говорит, что DTV «никогда не был должным образом определён и выпущен». Коммерческие
  аналоги тем не менее называют так тип экспорта и умеют превращать IFC-объекты DTV в
  собственные объекты.
- **Тенденция.** По политике buildingSMART роль обменных требований переходит к
  связке IDM + IDS. Базовые MVD остаются.
- **Приоритет.** RV — **P1**: заявить RV-совместимость экспорта и проверять её независимо.
  DTV — P3: импорт IFC в редактируемые балки и колонны Open BIM частично уже делает.

## 6. bSDD (buildingSMART Data Dictionary)

- **Что это.** Онлайн-сервис словарей: классы, свойства, допустимые значения, единицы, с
  URI вида `identifier.buildingsmart.org/uri/<org>/<dictionary>/<version>/class/<code>`.
  Словари публикуются в JSON по шаблону bSDD, у классов есть `RelatedIfcEntityNamesList`,
  у свойств — `PropertySet` (префикс `Pset_` зарезервирован за IFC). Для контрактов
  рекомендуется ссылаться на конкретную версию, а не на `latest`.
- **Кто использует.** Bonsai, коммерческие системы и плагины. Публикует ли российский КСИ свой
  словарь в bSDD, в этом сборе **не подтверждено**.
- **Лицензия.** Сервис бесплатный, у каждого словаря своя лицензия.
- **Приоритет: P3.** Достаточно сохранять в `IfcClassificationReference.Location` URI
  класса, если он есть.

## 7. DSTV-NC и DSTV-BV

- **DSTV-NC** — немецкий отраслевой стандарт (Deutscher Stahlbau-Verband) описания
  деталей для станков ЧПУ: файлы `.nc1`, по одному на позицию.
  - Блоки: ST (заголовок: заказ, чертёж, **фаза**, позиция, марка стали, количество,
    профиль, код профиля, длина, высота, ширина и толщина полки, толщина стенки,
    радиус, масса на метр, площадь окраски на метр, углы реза по стенке и полке),
    BO (отверстия), AK/IK (внешний и внутренний контур), SI (маркировка), PU/KO
    (разметка), SC (резы), TO, UE, PR, KA (гибка), IN, EN.
  - Коды профилей: I, U, L, M (прямоугольная труба), R (круглая), B (лист), T, C, SO (прочие).
  - Широко применяется **7-е издание (июль 1998)** с немецким текстом как
    авторитетным. Более новые редакции (иногда упоминаются DSTV-NC 2 и XML-варианты)
    в этом сборе **не подтверждены**.
- **Кто использует.** Все коммерческие системы деталировки экспортируют `.nc1`
  раздельно для пластин и профилей, с настройками знаков радиусов, кернения и
  маркировки контура (гнутые балки обычно не поддерживаются).
- **DSTV-BV / «DSTV Product interface».** Интерфейс DSTV для передачи стальной
  конструкции целиком (стержни для расчёта или сборки с обработкой), в коммерческих системах — файл `.STP`.
  Был ли это STEP и как называется текущая версия, в этом сборе **не подтверждено**.
- **Лицензия.** Описание стандарта распространяется DSTV / bauforumstahl, условия
  **не подтверждены**. Формат текстовый и реализуется свободно.
- **Rust:** крейт `dstv` (MIT, github.com/tluijken/dstv; 0.7.0 от 01.11.2024, есть 0.8.0)
  только читает `.nc1` и рисует SVG. Для **независимой проверки** экспорта он подходит,
  писатель придётся сделать самим.
- **Приоритет: P1** (строка `nc-dstv`, пункт 10 бэклога). Это минимальная передача на
  производство для стального приложения.

## 8. DXF / DWG

- **DXF** — открытое текстовое описание формата. Open BIM уже пишет DXF и
  проверяет его через ezdxf. Rust: `dxf` 0.6.2 (MIT; чтение и запись DXF/DXB).
- **DWG** — закрытый проприетарный формат. Открытых реализаций две:
  - **LibreDWG** (GNU, GPLv3; 0.13.3 от 26.02.2024 по Википедии, новее — не проверено).
    Читает все версии DWG, запись подтверждена для r2000. **GPLv3** не позволяет
    линковать его в приложение Apache-2.0 без перехода на GPL. Возможен вызов
    отдельной утилиты (`dwg2dxf`) как внешнего процесса — юридическую сторону этого нужно
    проверить отдельно;
  - **ODA Drawings SDK** (Open Design Alliance) — проприетарный SDK по членству
    (Commercial — до 100 копий, Sustaining и выше — без ограничений). Для открытого
    приложения он не подходит.
- **Приоритет: P2/P3** (строка `dwg`). Для КМ/КМД в России DXF достаточно как
  обменного формата, DWG нужен для подложек архитекторов. Импорт DXF-подложек — P2.

## 9. CIS/2 (CIMsteel Integration Standards)

- **Что это.** Стальной формат на базе STEP (ISO 10303), принятый AISC в 1998 году.
  Описывает анализ, проектирование и изготовление стали.
- **Статус.** AISC (2012) называет IFC долгосрочным путём, CIS/2 поддерживает, но
  развитие формата «застопорилось», сертификации нет. Актуальный статус на 2026 год
  **не подтверждён**. Современные коммерческие аналоги в справке по экспорту IFC CIS/2 не упоминают.
- **Приоритет: P3 / не делать.** Возможная ниша — обмен с американскими расчётными и
  MIS-системами. Вместо него лучше IFC4 RV + DSTV.

## 10. glTF 2.0 и xeokit XKT (просмотр)

- **glTF 2.0** (Khronos) — открытый royalty-free формат сцен. Опубликован как
  **ISO/IEC 12113:2022** (пресс-релиз Khronos от 04.08.2022). Нужен для лёгкого
  просмотра в вебе и на телефоне, без семантики IFC. Rust: `gltf` (крейт не проверялся
  в этом сборе).
- **xeokit XKT** — сжатый формат xeokit SDK (JS, WebGL, двойная точность координат).
  SDK под **AGPL-3.0**, коммерческая лицензия у Creoox AG. Конвертер `convert2xkt`
  принимает IFC, glTF, CityJSON, LAS/LAZ, PLY, STL. Пример из блога: IFC 45 МБ → XKT
  1,78 МБ. Наборы свойств лучше передавать через конвейер ifc2gltf.
- **That Open Engine:** web-ifc (MPL-2.0) + engine_components / engine_fragment (MIT),
  собственный формат Fragments.
- **Приоритет.** Экспорт glTF — **P2**: дешёвый «просмотр на площадке» без
  специализированного ПО. XKT — P3: если нужен веб-просмотрщик, проще отдавать IFC в
  That Open или xeokit, чем писать XKT.
