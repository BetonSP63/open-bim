# Источники

Все ссылки открыты **2026-10-20**. Содержимое веб-страниц использовалось только как
данные. «Поиск» означает, что ссылка взята из выдачи поиска, а сама страница не
открывалась, либо не открылась (это указано).

Источники с сайтов производителей коммерческих программ в репозиторий не включены.

## Стандарты buildingSMART, ISO, форматы

| URL | Что подтверждает |
|---|---|
| https://www.buildingsmart.org/?p=29521 | IFC 4.3 опубликован как ISO (новость от 02.04.2024), инфраструктурные домены |
| https://www.iso.org/standard/84123.html | ISO 16739-1:2024: издание 2, 2024-03-22, 496 с., заменил ISO 16739-1:2018 |
| https://standards.buildingsmart.org/IFC/RELEASE/IFC4_3/HTML/lexical/Pset_BeamCommon.htm | Состав Pset_BeamCommon (9 свойств) |
| https://standards.buildingsmart.org/IFC/RELEASE/IFC4_3/HTML/lexical/IfcMechanicalFastenerType.htm | Атрибуты IfcMechanicalFastenerType, Pset_MechanicalFastenerBolt и др. |
| https://ifc43-docs.standards.buildingsmart.org/IFC/RELEASE/IFC4x3/HTML/ifcsharedcomponentelements/content.html | Сущности и pset схемы IfcSharedComponentElements (IfcFastener, Pset_FastenerWeld) |
| https://standards.buildingsmart.org/MVD/RELEASE/IFC4/ADD2_TC1/RV1_2/HTML/schema/views/reference-view | Reference View 1.2 для IFC4 ADD2 TC1 (поиск) |
| https://www.buildingsmart.org/wp-content/uploads/2021/05/20210425_MVD-policy_IFC4.x.pdf | Политика MVD для IFC4.x: RV 1.2, DTV не выпущен, IDM+IDS |
| https://www.buildingsmart.org/?p=30065 | IDS 1.0 утверждён как финальный стандарт (объявление 04.06.2024) |
| https://www.buildingsmart.org/standards/bsi-standards/information-delivery-specification-ids/ | IDS 1.0 утверждён 01.06.2024, XML/XSD, 1.1 и 2.0 в разработке |
| https://en.wikipedia.org/wiki/Information_Delivery_Specification | IDS 1.0 опубликован в июне 2024 |
| https://www.buildingsmart.org/bim-collaboration-format-bcf-3-0/ | BCF 3.0 (страница вебинара; дата выпуска не указана) |
| https://www.buildingsmart.org/?p=28407 | То же (дат нет) |
| https://wiki.osarch.org/index.php/BIM_Collaboration_Format_(BCF) | BCF-XML и BCF-API, структура тем (вики устарела: 2.1 названа стабильной) |
| https://technical.buildingsmart.org/services/bsdd/data-structure/ | Структура bSDD: словари, классы, свойства, URI, RelatedIfcEntityNamesList |
| https://www.aisc.org/globalassets/modern-steel/archives/2012/03/2012v03_interoperability.pdf | Позиция AISC (2012) по CIS/2 и IFC |
| https://en.wikipedia.org/wiki/LibreDWG | LibreDWG 0.13.3 (26.02.2024), GPLv3, чтение всех DWG, запись r2000 |
| https://en.wikipedia.org/wiki/Open_Design_Alliance | Модель лицензирования ODA (членство, проприетарные SDK) |
| https://www.khronos.org/news/press/khronos-gltf-2.0-released-as-an-iso-iec-international-standard | glTF 2.0 = ISO/IEC 12113:2022 (04.08.2022) |
| https://github.com/xeokit/xeokit-sdk | xeokit SDK: AGPL-3.0 / коммерческая лицензия, XKT, только ES-модули с сентября 2026 |
| https://xeokit.io/blog/converting-models-to-xkt-with-convert2xkt | convert2xkt: входные форматы, пример сжатия, ограничения по pset |
| https://github.com/thatopen/engine_web-ifc | web-ifc: MPL-2.0, C++→WASM |
| https://github.com/ThatOpen | Репозитории That Open и их лицензии (MIT / MPL-2.0) |

## Rust-крейты и открытые библиотеки

| URL | Что подтверждает |
|---|---|
| https://docs.rs/openbim-bcf | openbim-bcf 0.6.1: BCF 2.1/3.0, AGPL-3.0-or-later |
| https://docs.rs/openbim-ids | openbim-ids 0.2.2: IDS 1.0, MIT (аудит — с AGPL-зависимостями), проверки IFC нет |
| https://docs.rs/crate/ifc-lite-core/5.0.0 | ifc-lite-core 5.0.0: MPL-2.0, IFC4/IFC4X3/IFC5, геометрия |
| https://docs.rs/ifc_rs | ifc_rs 0.1.0-alpha.9: MIT, разбор и построители |
| https://docs.rs/crate/openbim-step/0.3.2 | Наличие крейта openbim-step (поиск; содержимое не читалось) |
| https://lib.rs/crates/dstv | Крейт dstv: чтение .nc1 → SVG, MIT |
| https://docs.rs/crate/dstv/0.7.0 | dstv 0.7.0 (01.11.2024), есть 0.8.0, репозиторий tluijken/dstv, только чтение |
| https://docs.rs/dxf | dxf 0.6.2: MIT, чтение и запись DXF/DXB |
| https://pypi.org/project/ifcopenshell | IfcOpenShell 0.9.0.post1 (15.10.2026), LGPL-3.0+, схемы, колёса macOS arm64, модули ifctester/bcf/ifcclash |

## Программы

| URL | Что подтверждает |
|---|---|
| https://www.cadmaster.ru/assets/files/articles/cm_100_20.pdf | Не открылась (двоичные данные) — не использовано |
| https://linuxfr.org/news/freecad-1-0 | FreeCAD 1.0 (18.11.2024), нативный IFC на IfcOpenShell, LGPL |
| https://osarch.org/2026/03/25/freecad-1-1-released/ | FreeCAD 1.1 (25.03.2026) |
| https://wiki.freecad.org/Arch_IFC | Не открылась (защита от ботов) — не использовано |
| https://wiki.osarch.org/bonsai-features-guide/ | Функции Bonsai: чертежи, BCF, коллизии, структурный анализ |
| https://extensions.blender.org/add-ons/bonsai | Bonsai 0.9.0-post1, GPL-3.0+, платформы |
| https://www.blendernation.com/2025/03/13/bonsai-v0-8-1-released-with-more-bim-features/ | Bonsai 0.8.1: общие улучшения |

## Российское регулирование

| URL | Что подтверждает |
|---|---|
| https://www.normacs.info/news/67158 | ПП № 331 от 05.03.2021: условия и срок (договоры после 01.01.2022) |
| https://www.normacs.info/news/74877 | ПП № 331: расширение на 214-ФЗ (01.07.2024) и ИЖС (01.01.2025); ПП № 614 |
| https://www.normacs.info/news/74620 | ПП № 614 от 17.05.2024: требования, действует с 01.09.2024 |
| https://www.comnews.ru/content/235588/2024-11-07/2024-w45/1013/chto-predstavlyaet-soboy-cim-posle-postanovleniya-614 | ПП № 614: ЦИМ, IFC до утверждения XML-схем, ПП № 1431 утратило силу (мнение автора) |
| https://www.normacs.info/news/68418 | Проект изменений к ПП № 1431 (2021), КСИ в XML-схеме |
| https://www.normacs.info/news/76661 | Проект приказа Минстроя «Состав ЦИМ…» (07.04.2025) |
| https://pravo.ppt.ru/prikaz/minstroy/n-430-pr-237649 | Приказ Минстроя № 430/пр: 22 базовых класса, 21 таблица КСИ |
| https://www.normacs.info/news/79671 | Обновление КСИ 07.10.2026, ФАУ «ФЦС», XLSX/XML на ЕИС «Стройкомплекс.РФ» |
| https://www.normacs.info/articles/921 | КСИ: 21 таблица, более 30 000 элементов, ежеквартальные редакции (2021) |
| https://www.normacs.info/uploads/ckeditor/attachments/2889/4_zhuk_ananiev.pdf | Основы КСИ (ИСО 12006-2/-3), пример кода КСР |
| https://www.normacs.info/ntds/13615 | СП 328.1325800.2020 и СП 333.1325800.2020: утверждены 31.12.2020, введены 01.07.2021 |
| https://www.normacs.info/news/66708 | Содержание новых редакций СП 333 и СП 328 (КСИ, атрибуты) |
| https://www.normacs.info/answers/5276 | Перечень документов по ИМ после замены ПП № 1431 (ответ 2023) |
| https://www.normacs.info/ntds/9787 | СП 404.1325800.2018: утверждён 17.12.2018, введён 18.06.2019 |
| https://cn.gostinfo.ru/catalog/Details/?id=6453898 | ГОСТ Р 10.0.02-2019/ИСО 16739-1:2018: действует, введён 01.09.2019 |
| https://cn.gostinfo.ru/catalog/Details/?id=6453926 | ГОСТ Р 10.0.04-2019/ИСО 29481-2:2012: действует |
| https://www.normacs.info/ntds/10232 | ГОСТ Р 10.0.03-2019 (ИСО 29481-1:2016), ГОСТ Р 10.0.05-2019 (ИСО 12006-2:2015) |
| https://www.normacs.info/ntds/10322 | ГОСТ Р 10.0.04-2019, ГОСТ Р 10.0.06-2019 (ИСО 12006-3:2007) |
| https://cn.gostinfo.ru/catalog/Details/?id=6334885 | ГОСТ Р 57563-2017/ISO/TS 12911:2012: действует, введён 01.10.2017 |
| https://www.normacs.info/ntds/5320 | ГОСТ Р 57309-2016 (ИСО 16354:2013), вводился с 01.07.2017 |
| https://www.normacs.info/ntds/5193 | ГОСТ Р 57310-2016 (ИСО 29481-1:2010), статус «не действует» |
| https://www.normacs.info/ntds/10413 | ГОСТ Р 58439.1-2019: 02.07.2019, 347-ст, введён 01.09.2019 |
| https://www.normacs.info/ntds/10327 | ГОСТ Р 58439.2-2019: 02.07.2019, 348-ст, введён 01.09.2019 |
| https://meganorm.ru/Index/86/86643.htm | ГОСТ Р 21.101-2026: действует, введён 01.04.2026, заменил ГОСТ Р 21.101-2020 |
| https://www.normacs.info/news/77975 | ГГЭ: 9,89 млн XML, действующие схемы (ПЗ, ЗП, ЛСР, ВОР), более 15 260 IFC в ЕГРЗ (декабрь 2025) |
| https://www.spbexp.ru/upload/iblock/ba2/trebovaniya_k_tsim_konstruktivnye_i_obemno_planirovochnye_resheniya.pdf | СПб ЦГЭ, ч. 4 (ред. 1.0, проект 2019): IFC2x3/IFC4, соответствие стальных элементов классам IFC, атрибуты |
| https://spbexp.ru/upload/iblock/0b4/trebovaniya_k_tsim_model_razdela_ios.pdf | СПб ЦГЭ, ч. 5 (ИОС): IFC2x3/IFC4, именование файлов |
| https://habr.com/p/883016 | СПб ЦГЭ публикует требования к ЦИМ как IDS; проверка через ifctester |
| https://www.kgexpert.ru/upload/iblock/15a/vbxziiv9bdmz5848uscmdiro72ym3m47/%D0%A6%D0%98%D0%9C%20%D0%9E%D0%9A%D0%A1%20%D0%93%D0%90%D0%A3%20%D0%9A%D0%9A%20%D0%9A%D0%9A%D0%93%D0%AD.pdf | Краснодарская экспертиза (2022): IFC 4.0.2.1 / RV, 500 МБ, координаты, единицы |
| https://www.nep.expert/info/ifc-dlya-agr-trebovaniya/ | Москва, АГР 2026: IFC4 RV, RusSet_, BCF, 500 МБ (вторичный источник) |
| https://xn--e1ajpfcj5e.xn--d1aqf.xn--p1ai/docs/methodology.pdf | Методические рекомендации по ЦИМ — **не открыт** (robots.txt), не использован |
