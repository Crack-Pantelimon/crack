#!/usr/bin/env python3
import os
import sys
import json
import subprocess

# Define the output paths
BASE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MISSIONS_DIR = os.path.join(BASE_DIR, "docs", "missions")
CHARACTERS_DIR = os.path.join(BASE_DIR, "docs", "characters")
DOCS_DIR = os.path.join(BASE_DIR, "docs")
PROMPTS_DIR = os.path.join(BASE_DIR, "_slop", "prompts")

# Data for characters (Wikipedia style)
CHARACTERS = {
    "relu_oncescu": {
        "title": "Relu Oncescu - Wikipedia",
        "name": "Relu Oncescu",
        "role": "Protagonist / Taximetrist / Recuperator",
        "description": "Relu este protagonistul principal al universului 'Vice City: Pantelimon'. Aparent un taximetrist banal din București, Relu lucrează în secret ca recuperator de datorii pentru Căpitanu', un cap al mafiei locale din Pantelimon. Locuiește într-un apartament de bloc cu soția sa, Gina, și cei doi copii ai lor, Magda și Chuckie. Este o persoană extrem de calculată, tăcută, dar violentă atunci când situația o cere.",
        "biography": "Născut și crescut în Pantelimon, Relu a învățat devreme regulile străzii. După ce a făcut armata și a încercat să aibă o viață normală, lipsa banilor l-a împins către lumea interlopă. A devenit omul de încredere al Căpitanului datorită loialității și eficienței sale. Își ascunde activitatea criminală sub paravanul unei licențe de taxi pe o Dacia Logan galbenă.",
        "relationships": "- **Gina Oncescu**: Soția sa, care crede că el este doar taximetrist și devine extrem de suspicioasă.\n- **Căpitanu'**: Șeful său direct, cel care îi ordonă recuperările.\n- **Teddy**: Fiul Căpitanului, pe care Relu trebuie să-l dădăcească și care devine iubitul fiicei sale, Magda.\n- **Nico**: Colega de rețea, care administrează plățile și cu care are o relație complicată de camaraderie/tensiune sexuală.\n- **Nea Puiu**: Unchiul și mentorul său informal, un veteran al lumii interlope din anii '90.",
        "trivia": "- Conduce o Dacia Logan galbenă cu motor de 1.4 MPI pe GPL.\n- Preferă să rezolve disputele cu un baros sau cu pumnii, evitând armele de foc până când situația scapă complet de sub control.\n- Are un antrenament riguros la o sală de box ascunsă în subsolul unui bloc din Pantelimon."
    },
    "gina_oncescu": {
        "title": "Gina Oncescu - Wikipedia",
        "name": "Gina Oncescu",
        "role": "Soția lui Relu / Casnică",
        "description": "Gina este soția lui Relu. Este o femeie gospodină, dedicată familiei, dar extrem de suspicioasă cu privire la programul nocturn al soțului ei și la parfumul de damă străin pe care îl simte uneori pe hainele lui.",
        "biography": "Gina s-a căsătorit cu Relu de tânără, crezând că va avea o viață liniștită alături de un mecanic/taximetrist muncitor. Pe măsură ce secretele lui Relu se adună, viața ei de familie se destramă, culminând cu descoperirea adevărului în Sezonul 1.",
        "relationships": "- **Relu Oncescu**: Soțul ei.\n- **Magda și Chuckie**: Copiii ei, pe care încearcă să-i protejeze de influența străzii.\n- **Sabin**: Fratele ei, un tip cam bătut în cap pe care Relu îl bate după ce acesta încearcă să-l urmărească.",
        "trivia": "- Face cele mai bune sarmale din Pantelimon.\n- Cumpărăturile ei preferate sunt făcute la Cora Pantelimon, unde se petrec și câteva scene cheie."
    },
    "capitanu": {
        "title": "Căpitanu' (Doru) - Wikipedia",
        "name": "Căpitanu'",
        "role": "Antagonist / Șef de Clan",
        "description": "Căpitanu' este liderul clanului interlop care controlează afacerile ilegale din sectorul 2, în special în Pantelimon. Este un bărbat corpolent, violent și obsedat de control, care își desfășoară activitatea dintr-un restaurant/terasă locală.",
        "biography": "Fost bișnițar în anii '90, Căpitanu' a urcat în ierarhia interlopă prin cruzime și alianțe strategice. Controlează prostituția, cămătăria și taxele de protecție din estul capitalei.",
        "relationships": "- **Relu Oncescu**: Principalul său enforcer.\n- **Teddy**: Fiul său, pe care vrea să-l transforme într-un interlop adevărat.\n- **Nico**: Asistenta și contabila sa.\n- **Toma**: Mafiotul din Constanța cu care are relații tensionate de afaceri.",
        "trivia": "- Este obsedat de mâncarea tradițională românească și dă ordine în timp ce mănâncă mici cu muștar.\n- Nu suportă să fie contrazis și are crize de furie în care sparge pahare."
    },
    "nico": {
        "title": "Nico (Nicoleta) - Wikipedia",
        "name": "Nico",
        "role": "Locotenent / Coordonator Clan",
        "description": "Nico este mâna dreaptă a Căpitanului, responsabilă cu evidența banilor, coordonarea fetelor de pe centură și transmiterea ordinelor către Relu. Este o femeie atrăgătoare, cinică și extrem de calculată.",
        "biography": "Intrată de tânără în anturajul Căpitanului, Nico și-a folosit inteligența pentru a deveni indispensabilă. Joacă un joc periculos, fiind prinsă între loialitatea față de clan și dorința de a scăpa din această lume.",
        "relationships": "- **Relu Oncescu**: Partener de afaceri și confident.\n- **Căpitanu'**: Șeful ei.\n- **Emilian**: Inspectorul de poliție care o șantajează pentru a deveni informatoare.",
        "trivia": "- Conduce un SUV negru de lux.\n- Folosește un parfum scump care o dă de gol în fața Ginei."
    },
    "teddy": {
        "title": "Teddy (Theodor) - Wikipedia",
        "name": "Teddy",
        "role": "Fiul Căpitanului / Partenerul lui Relu",
        "description": "Teddy este fiul Căpitanului. Spre deosebire de tatăl său, Teddy este mai sensibil, educat și nu are profilul unui interlop clasic. Tatăl său îl trimite cu Relu 'la ucenicie' pentru a se căli.",
        "biography": "Crescut în puf, dar sub presiunea constantă a unui tată abuziv, Teddy încearcă să-și dovedească bărbăția. Se îndrăgostește de Magda, fiica lui Relu, ceea ce complică masiv alianțele din joc.",
        "relationships": "- **Căpitanu'**: Tatăl său autoritar.\n- **Relu Oncescu**: Mentorul său pe teren.\n- **Magda Oncescu**: Iubita și ulterior soția sa.",
        "trivia": "- Îi place muzica rock, spre disperarea tatălui său care ascultă manele.\n- Devine mult mai violent în sezoanele 2 și 3, după ce este forțat de circumstanțe."
    },
    "magda_oncescu": {
        "title": "Magda Oncescu - Wikipedia",
        "name": "Magda Oncescu",
        "role": "Fiica lui Relu",
        "description": "Magda este fiica adolescentă a lui Relu și a Ginei. Este o fire rebelă care dorește să scape de sub controlul părinților și de atmosfera sufocantă din apartamentul de bloc.",
        "biography": "Elevă la un liceu din sectorul 2, Magda îl cunoaște pe Teddy într-o cafenea din Pantelimon. Relația lor se transformă rapid într-o poveste de dragoste complicată, ea rămânând însărcinată la finalul primului sezon.",
        "relationships": "- **Relu și Gina**: Părinții ei.\n- **Teddy**: Iubitul/soțul ei.\n- **Chuckie**: Fratele mai mic.",
        "trivia": "- Adoră să meargă în cluburi din centrul vechi, dar sfârșește prin a fi prinsă în războiul clanurilor."
    },
    "chuckie_oncescu": {
        "title": "Chuckie Oncescu (Codrin) - Wikipedia",
        "name": "Chuckie Oncescu",
        "role": "Fiul lui Relu",
        "description": "Chuckie este fiul cel mic al lui Relu. Este pasionat de jocuri video și biciclete, fiind destul de deconectat de lumea reală până când problemele tatălui său îi bat la ușă.",
        "biography": "Elev de gimnaziu, are frecvent probleme la școală din cauza notelor și a altercațiilor cu colegii. Este foarte atașat de tatăl său, pe care îl consideră un erou, fără să știe ce face de fapt.",
        "relationships": "- **Relu și Gina**: Părinții săi.\n- **Magda**: Sora sa.",
        "trivia": "- Se joacă GTA pe calculator în camera lui, o glumă meta adusă în joc."
    },
    "nea_puiu": {
        "title": "Nea Puiu - Wikipedia",
        "name": "Nea Puiu",
        "role": "Veteran / Pensionar Interlop",
        "description": "Nea Puiu este un fost infractor din garda veche, rudă cu Relu, care își petrece timpul în sala de box a lui Relu. Este nostalgic după anii '90 și plin de povești nebunești.",
        "biography": "După ce a ispășit mai mulți ani de închisoare, s-a retras din activitatea principală, dar mintea lui este încă plină de scheme criminale. Devine o problemă când începe să sufere de episoade paranoice și demență ușoară.",
        "relationships": "- **Relu Oncescu**: Nepotul și protectorul său.\n- **Gina**: Pe care o enervează constant.",
        "trivia": "- Are mereu la el un cuțit vechi și povestește cum se făceau 'mărunțișurile' pe vremuri."
    },
    "emilian": {
        "title": "Emilian - Wikipedia",
        "name": "Emilian",
        "role": "Principalul Antagonist / Polițist Psihopat",
        "description": "Emilian este un inspector de poliție sosit special pentru a destructura rețeaua Căpitanului. Este sadic, psihopat și folosește metode ilegale (tortură, șantaj, plantare de probe) pentru a-și atinge scopurile.",
        "biography": "Un gabor de temut, Emilian nu respectă nicio lege. Devine obsedat de Relu, realizând că acesta este piesa centrală din puzzle-ul mafiei din sectorul 2.",
        "relationships": "- **Relu Oncescu**: Inamicul său jurat.\n- **Nico**: Pe care o folosește ca informatoare prin șantaj.\n- **Căpitanu'**: Ținta sa principală în primele sezoane.",
        "trivia": "- Are un tic verbal de superioritate și îi place să își tortureze victimele psihic înainte de a le bate."
    },
    "toma": {
        "title": "Toma - Wikipedia",
        "name": "Toma",
        "role": "Boss de Constanța",
        "description": "Toma este liderul suprem al mafiei din Constanța, un bătrân deosebit de periculos, sobru și influent, care controlează contrabanda din portul Constanța.",
        "biography": "Un personaj de temut la nivel național, Toma colaborează cu Căpitanu' pentru spălarea de bani și distribuția de droguri, dar privirea lui se îndreaptă rapid spre preluarea totală a Bucureștiului.",
        "relationships": "- **Relu Oncescu**: Cu care face o înțelegere secretă.\n- **Căpitanu'**: Partenerul său din București pe care îl disprețuiește.",
        "trivia": "- Locuiește într-o vilă masivă pe malul mării și vorbește pe un ton șoptit, dar letal."
    },
    "nicu": {
        "title": "Nicu - Wikipedia",
        "name": "Nicu",
        "role": "Locotenent / Mobster de Spania",
        "description": "Nicu este un interlop român care a activat mulți ani în Spania. Se întoarce în țară cu idei noi, violente și dorește să preia controlul teritoriului administrat de Căpitanu'.",
        "biography": "Repatriat cu bani mulți și conexiuni externe, Nicu nu are respect pentru ierarhia locală veche din Pantelimon. Declanșează un război sângeros în Sezonul 2.",
        "relationships": "- **Căpitanu'**: Pe care vrea să-l înlocuiască.\n- **Relu Oncescu**: Rivalul său direct pe străzi.",
        "trivia": "- Poartă haine de designer ostentative și vorbește cu inserții de cuvinte în spaniolă (hola, jefe, dinero)."
    },
    "sabin": {
        "title": "Sabin - Wikipedia",
        "name": "Sabin",
        "role": "Cumnatul lui Relu / Comic Relief",
        "description": "Sabin este fratele Ginei, un bărbat simplu și naiv care bănuiește că Relu se ocupă cu ceva ilegal și încearcă să-l spioneze, terminând prin a fi snopit în bătaie de Relu fără ca Gina să știe cine l-a lovit.",
        "biography": "Sabin este fratele Ginei. Nu este prea inteligent și lucrează ca taximetrist independent sau face diverse munci mărunte prin sectorul 2. Este convins că Relu are o amantă sau face chestii ilegale și încearcă să fie detectiv de cartier, dar sfârșește prin a fi bătut.",
        "relationships": "- **Gina**: Sora lui.\n- **Relu**: Cumnatul pe care îl suspectează.",
        "trivia": "- Conduce o rablă de mașină și se crede detectiv privat."
    }
}

# 42 missions split by season
# Season 1: 1-16
# Season 2: 17-28
# Season 3: 29-42
MISSIONS = []

# Helper to generate mission template
def add_mission(num, title, giver, rewards, prereq, obj, desc, dialogue, storyboard, next_m):
    MISSIONS.append({
        "num": num,
        "title": title,
        "giver": giver,
        "rewards": rewards,
        "prereq": prereq,
        "obj": obj,
        "desc": desc,
        "dialogue": dialogue,
        "storyboard": storyboard,
        "next": next_m
    })

# Define the 42 missions
# --- SEZONUL 1 ---
add_mission(
    1, "Taximetria pe GPL", "Nico", "100 EUR, Respect +10", "Niciuna (Misiune de început)",
    ["Ia clienții din stația de la Cora Pantelimon.", "Du-i la destinație în siguranță.", "Răspunde la apelul telefonic de la Nico."],
    "Relu își începe ziua conducând Dacia Logan galbenă în zona Pantelimon. Trebuie să transporte 3 clienți diferiți, fiecare având replici tipice românești (un pensionar revoltat de prețuri, un corporatist grăbit și un bețiv prietenos). După finalizarea curselor, Nico îl sună pe Relu și îi spune că are o treabă de recuperare la barul lui nea Sandu de lângă Spitalul Pantelimon.",
    "- Client 1 (pensionar): 'Băi băiatule, pe vremea mea biletul de tramvai era 50 de bani, tu-mi ceri 20 de lei până la Delfinului?!'\n- Nico: 'Relu, lasă clienții ăia amărâți. Avem o problemă la barul lui Sandu. Mișcă-te!'",
    ["[Cadru 1] Relu stă la volanul Daciei Logan galbene, uitându-se plictisit la ceasul de la Cora Pantelimon.",
     "[Cadru 2] Un pensionar cu sacoșe de rafie urcă pe bancheta din spate bodogănind.",
     "[Cadru 3] Relu primește un telefon pe un Nokia 3310 vechi, fața lui devenind brusc serioasă."],
    "Misiunea 2: Recuperare de Noapte"
)

add_mission(
    2, "Recuperare de Noapte", "Nico", "300 EUR, Pumnal", "Misiunea 1: Taximetria pe GPL",
    ["Mergi la barul lui nea Sandu în Pantelimon.", "Intimidează-l pe Sandu pentru a plăti datoria de 2000 EUR.", "Ascunde corpul agresorului în portbagaj.", "Aruncă corpul în Lacul Pantelimon."],
    "Relu ajunge la barul lui Sandu. Acesta refuză să plătească și îl atacă pe Relu cu o bâtă. Relu îl pune la pământ cu câțiva pumni bine plasați. Unul dintre oamenii lui Sandu sare la bătaie, dar Relu îl lovește prea tare și acesta rămâne inert. Realizând că l-a omorât din greșeală, Relu îl pune în portbagajul Daciei și conduce până la marginea lacului Pantelimon pentru a scăpa de cadavru sub acoperirea nopții.",
    "- Sandu: 'N-am banii, băi Relule! Spune-i Căpitanului că-i dau săptămâna viitoare!'\n- Relu: 'Căpitanu' nu așteaptă. Iar eu nu-mi bat gura degeaba.'",
    ["[Cadru 1] Relu intră în barul întunecat și plin de fum, unde Sandu stă speriat la masă.",
     "[Cadru 2] Luptă corp la corp: Relu îi dă un croșeu de stânga unui bodyguard, care cade secerat în spatele tejghelei.",
     "[Cadru 3] Relu trage trupul inert pe pământul noroios de lângă Lacul Pantelimon, sub lumina palidă a Lunii."],
    "Misiunea 3: O Escortă de Protejat"
)

add_mission(
    3, "O Escortă de Protejat", "Nico", "400 EUR, Pistol 9mm", "Misiunea 2: Recuperare de Noapte",
    ["Mergi la hotelul 'Lebăda' din Pantelimon.", "Asigură paza fetelor Căpitanului.", "Elimină bodyguarzii clanului rival din Ferentari."],
    "Nico îl trimite pe Relu să rezolve o dispută la un hotel local unde fetele de sub protecția Căpitanului sunt hărțuite de interlopi din Ferentari. Jucătorul trebuie să folosească tehnici de stealth sau luptă deschisă pentru a curăța hotelul de intrusi, demonstrând că Pantelimonul aparține Căpitanului.",
    "- Nico: 'Vezi că băieții ăia de la Ferentari au cam trecut granița. Du-te și arată-le unde le e locul.'\n- Relu: 'Se rezolvă. Fără zgomot.'",
    ["[Cadru 1] Relu coboară din Logan în parcarea hotelului, verificându-și pistolul sub geacă.",
     "[Cadru 2] Relu îl prinde pe la spate pe un paznic rival în holul hotelului.",
     "[Cadru 3] Fetele stau speriate într-un colț al camerei în timp ce Relu curăță zona."],
    "Misiunea 4: Băiatul Șefului"
)

add_mission(
    4, "Băiatul Șefului", "Căpitanu'", "200 EUR, Respect +20", "Misiunea 3: O Escortă de Protejat",
    ["Mergi la restaurantul Căpitanului.", "Ia-l pe Teddy în mașină.", "Mergi la șantierul de lângă Șoseaua Fundeni.", "Asistă la colectarea banilor și protejează-l pe Teddy."],
    "Căpitanu' îl cheamă pe Relu și îi cere să-l ia pe fiul său, Teddy, pe teren pentru a-l învăța meserie. Merg la un constructor care îi datorează bani Căpitanului. Când constructorul încearcă să-l păcălească pe Teddy, acesta se blochează, iar Relu intervine brutal. După misiune, îl lasă pe Teddy la o cafenea din Pantelimon, unde acesta o întâlnește pe Magda (fiica lui Relu), fără ca vreunul să știe conexiunea de familie.",
    "- Căpitanu': 'Fă-l bărbat, Relule. E prea moale. Ascultă rock în loc să se ocupe de afaceri.'\n- Teddy: 'Nu sunt moale, tată...'\n- Relu: 'Urcă în mașină, puștiule.'",
    ["[Cadru 1] Căpitanu' mănâncă mici de pe o farfurie de carton și îi vorbește lui Relu, în timp ce Teddy stă supărat în colț.",
     "[Cadru 2] Pe șantier, Relu trântește un constructor pe o grămadă de nisip, în timp ce Teddy privește îngrozit.",
     "[Cadru 3] Teddy o vede pe Magda citind la o masă pe o terasă și se apropie sfiit."],
    "Misiunea 5: Doi la Preț de Unul"
)

add_mission(
    5, "Doi la Preț de Unul", "Căpitanu'", "500 EUR, Uzi", "Misiunea 4: Băiatul Șefului",
    ["Ia-l pe Teddy de la sala de box.", "Mergi la Piața Obor.", "Recuperează taxa de la tarabagiii de flori.", "Scapă de gaborii care patrulează zona."],
    "Relu și Teddy merg la Piața Obor pentru a strânge taxa săptămânală de la florari. Câțiva bișnițari locali refuză și cheamă poliția locală. Relu și Teddy trebuie să fugă printre tarabe, să urce în Logan și să scape de poliție printr-o urmărire intensă pe străduțele înguste din spatele pieței.",
    "- Teddy: 'Relu, vin gaborii! Ce facem?'\n- Relu: 'Calm. Urcă în mașină și ține-te bine de mâner.'",
    ["[Cadru 1] Vânzătoarele de flori țipă în timp ce Relu răstoarnă o tarabă pentru a bloca gaborii.",
     "[Cadru 2] Loganul galben virează strâns pe două roți pe o străduță plină de gropi din Obor.",
     "[Cadru 3] Teddy zâmbește plin de adrenalină după ce au scăpat de mașina de poliție."],
    "Misiunea 6: Fiorul Dragostei"
)

add_mission(
    6, "Fiorul Dragostei", "Teddy", "300 EUR", "Misiunea 5: Doi la Preț de Unul",
    ["Du-l pe Teddy să cumpere flori din Obor.", "Condu-l la întâlnirea cu Magda în Parcul Cosmos.", "Apără-l pe Teddy de golanii din parc."],
    "Teddy îl roagă pe Relu să-l ajute cu o treabă personală: vrea să meargă la o întâlnire cu Magda în Parcul Cosmos. Jucătorul îl conduce pe Teddy acolo. În timp ce ei vorbesc, niște golani din Pantelimon se iau de ei. Relu, ascuns după niște tufe, trebuie să intervină discret și să-i bată pe golani fără ca Magda să realizeze că tatăl ei este cel care îi protejează din umbră.",
    "- Teddy: 'Relu, te rog nu-i spune tatălui meu. O să creadă că sunt un fraier.'\n- Relu: 'Nu-i spun. Dar ai grijă de tine.'",
    ["[Cadru 1] Teddy îi oferă Magdei un buchet mare de trandafiri pe o bancă în Parcul Cosmos.",
     "[Cadru 2] Trei golani în treninguri Adidas se apropie amenințător de cei doi.",
     "[Cadru 3] Relu îl lovește pe la spate pe unul din golani cu o cheie franceză mare, trăgându-l în boscheți."],
    "Misiunea 7: Suspiciuni de Soție"
)

add_mission(
    7, "Suspiciuni de Soție", "Gina", "150 EUR", "Misiunea 6: Fiorul Dragostei",
    ["Du-te acasă la Gina.", "Mergi la magazinul Cora Pantelimon.", "Urmărește-l pe Sabin (fratele Ginei) care te spionează.", "Dezactivează mașina lui Sabin fără să fii văzut."],
    "Gina este convinsă că Relu are o amantă din cauza parfumului de pe hainele sale (parfumul lui Nico). Îl trimite pe fratele ei, Sabin, să-l urmărească. Relu observă că este urmărit de o rablă de mașină în timp ce merge la Cora. Jucătorul trebuie să-și piardă urma, apoi să se furișeze în spatele mașinii lui Sabin și să-i taie cablurile de la motor pentru a-i opri urmărirea.",
    "- Gina: 'De unde miroși așa, Relule? Iar ai reparat mașina vreunei dudui?'\n- Relu: 'E de la odorizantul de taxi, Gina. Lasă-mă în pace.'",
    ["[Cadru 1] Gina stă cu mâinile în șolduri în bucătărie, certându-l pe Relu care mănâncă ciorbă.",
     "[Cadru 2] Sabin privește printr-un binoclu dintr-o Dacia veche parcată la colțul blocului.",
     "[Cadru 3] Relu sabotează motorul mașinii lui Sabin cu un clește, zâmbind ironic."],
    "Misiunea 8: Afaceri de Familie"
)

add_mission(
    8, "Afaceri de Familie", "Nico", "600 EUR, Shotgun", "Misiunea 7: Suspiciuni de Soție",
    ["Mergi la depozitul de la Granitul.", "Prelucrează transportul de țigări de contrabandă.", "Snopeste-l în bătaie pe Sabin la colțul blocului."],
    "Nico organizează un transport important de țigări la depozitul de la Granitul. Relu coordonează descărcarea. La întoarcere, Relu îl prinde din nou pe Sabin încercând să-i spioneze casa. De data aceasta, Relu îl bate crunt pe Sabin într-un colț întunecat pentru a-i da o lecție definitivă, având grijă să nu-și arate fața pentru ca Gina să nu afle cine l-a bătut pe fratele ei.",
    "- Nico: 'Marfa asta de contrabandă trebuie să ajungă în Obor până dimineață. Fără greșeli.'\n- Sabin: 'Au! Cine ești, mă? Nu da! Moare mama!'",
    ["[Cadru 1] Relu și alți băieți descarcă cartoane de țigări dintr-un tir ascuns în depozitul Granitul.",
     "[Cadru 2] În spatele blocului, Relu îi pune un sac pe cap lui Sabin și începe să-l lovească cu pumnii.",
     "[Cadru 3] Sabin zace plin de sânge lângă ghena de gunoi, în timp ce Relu pleacă calm în noapte."],
    "Misiunea 9: Doctorul vine la Bloc"
)

# --- SEZONUL 1 CONTINUARE ---
add_mission(
    9, "Doctorul vine la Bloc", "Căpitanu'", "800 EUR", "Misiunea 8: Afaceri de Familie",
    ["Mergi la sala de box a lui Relu.", "Curăță sala de arme și droguri ascunse.", "Întâmpină-l pe 'Doctorul' și arată-i respect."],
    "Căpitanu' anunță că 'Doctorul', un asociat periculos și influent de la Constanța, vine în vizită pentru a inspecta facilitățile. Relu trebuie să meargă rapid la sala sa de box din subsolul blocului și să mute toate armele și pachetele suspecte într-un apartament vecin înainte ca Doctorul să ajungă. Urmează o scenă de dialog tensionată în care orice greșeală de răspuns poate fi fatală.",
    "- Căpitanu': 'Vine Doctorul, Relule. Dacă găsește ceva în neregulă, ne curăță pe toți.'\n- Doctorul: 'Interesant loc ai aici, Relu. Sper că ești la fel de curat pe cât pari.'",
    ["[Cadru 1] Relu aruncă saci plini de arme printr-o fereastră de subsol direct în portbagajul taxiului.",
     "[Cadru 2] Un bărbat elegant, la costum (Doctorul), coboară dintr-o limuzină neagră în fața blocului gri.",
     "[Cadru 3] Doctorul își trece degetul peste un sac de box, privind fix în ochii lui Relu."],
    "Misiunea 10: Acasă la Relu"
)

add_mission(
    10, "Acasă la Relu", "Gina", "0 EUR (Misiune de Poveste)", "Misiunea 9: Doctorul vine la Bloc",
    ["Intră în apartamentul lui Relu.", "Confruntă-te cu Gina și Sabin cel bătut.", "Adună-ți lucrurile personale.", "Mergi la safehouse-ul lui Nea Puiu."],
    "Sabin o convinge pe Gina că Relu l-a bătut și că este implicat în mafia locală. Când Relu ajunge acasă, Gina îl așteaptă cu bagajele făcute și îl dă afară din casă într-o scenă extrem de dramatică. Relu trebuie să-și adune hainele și restul banilor ascunși în perete și să se mute temporar la garajul insalubru al lui Nea Puiu.",
    "- Gina: 'Pleacă! Nu vreau să te mai văd în casa mea! Ești un monstru, Relule! Uită-te ce i-ai făcut lui Sabin!'\n- Relu: 'Gina, nu-i ceea ce crezi...'\n- Gina: 'Ieși!'",
    ["[Cadru 1] Gina plânge în hohote aruncând hainele lui Relu pe holul blocului.",
     "[Cadru 2] Relu scoate un teanc de euro ascuns în spatele unei prize demontate.",
     "[Cadru 3] Relu stă pe o canapea ruptă în garajul lui Nea Puiu, înconjurat de scule auto și praf."],
    "Misiunea 11: Probleme la Școală"
)

add_mission(
    11, "Probleme la Școală", "Gina", "100 EUR, Respect +10", "Misiunea 10: Acasă la Relu",
    ["Mergi la școala generală din Pantelimon.", "Întâlnește-te cu Gina și diriginta lui Chuckie.", "Urmărește-l pe tatăl agresorului lui Chuckie.", "Intimidează-l pe polițist în trafic."],
    "Deși sunt despărțiți, Gina îl sună pe Relu pentru că fiul lor, Chuckie, este agresat la școală de un coleg al cărui tată este un gabor (polițist) corupt și influent. Relu și Gina merg la școală. Ulterior, Relu îl urmărește pe polițist în trafic și folosește mașina pentru a-l tampona ușor și a-l amenința cu barosul, explicându-i că băiatul lui trebuie să-l lase în pace pe Chuckie.",
    "- Diriginta: 'Codrin (Chuckie) este agresiv, dar și celălalt băiat îl provoacă. Tatăl lui e inspector de poliție...'\n- Relu (către polițist): 'Dacă mai plânge băiatul meu o singură dată, Loganul ăsta galben o să treacă peste mașina ta. Ai înțeles?'",
    ["[Cadru 1] Relu stă în cabinetul directoarei lângă o Gina rece și distantă.",
     "[Cadru 2] Relu blochează mașina polițistului într-o intersecție aglomerată din Pantelimon.",
     "[Cadru 3] Relu coboară geamul și îi arată polițistului un baros greu, privindu-l amenințător."],
    "Misiunea 12: Nemulțumirea Căpitanului"
)

add_mission(
    12, "Nemulțumirea Căpitanului", "Căpitanu'", "700 EUR, Micro-SMG", "Misiunea 11: Problems la Școală",
    ["Mergi la biroul Căpitanului.", "Interceptă camionul cu marfă pe Centura București.", "Elimină paza camionului.", "Condu camionul la depozitul din Pantelimon."],
    "Căpitanu' este extrem de nervos pentru că o livrare de electronice furate a fost interceptată sau întârziată. Îl trimite pe Relu să o recupereze cu forța de pe Centura București, unde este păzită de o firmă privată de securitate care colaborează cu rivalii. O misiune clasică de atac în mișcare și condus vehicule grele.",
    "- Căpitanu': 'Marfa aia valorează 50 de mii de euro. O vreau în depozit în 2 ore, altfel te îngrop eu pe tine!'\n- Relu: 'O rezolv. Pregătește oamenii să descarce.'",
    ["[Cadru 1] Relu conduce Loganul umăr la umăr cu un camion mare pe șoseaua de centură.",
     "[Cadru 2] Relu trage cu un micro-SMG pe geamul Loganului spre cabina camionului.",
     "[Cadru 3] Camionul intră în viteză pe porțile depozitului Căpitanului, sub privirile mulțumite ale interlopilor."],
    "Misiunea 13: Încolțit"
)

add_mission(
    13, "Încolțit", "Nico", "500 EUR", "Misiunea 12: Nemulțumirea Căpitanului",
    ["Furișează-te în secția de poliție locală.", "Șterge înregistrările video de pe server.", "Evită camerele și patrulele de poliție."],
    "Relu află de la Nico că poliția (condusă de un inspector ciudat pe nume Emilian) a obținut imagini video cu Dacia lui Logan în apropierea locului unde a fost aruncat cadavrul de la barul lui Sandu. Relu trebuie să se infiltreze noaptea în secția de poliție locală pentru a distruge hard-diskul cu înregistrările camerelor de supraveghere.",
    "- Nico: 'Gaborii au filmări cu mașina ta la lac. Dacă nu le ștergi acum, ești istorie.'\n- Relu: 'Intru prin spate. Ține-mă la curent pe stație.'",
    ["[Cadru 1] Relu escaladează un gard de sârmă ghimpată în spatele secției de poliție.",
     "[Cadru 2] Relu se ascunde după un dulap de fișiere în timp ce un polițist trece prin coridor.",
     "[Cadru 3] Relu smulge hard-diskurile dintr-un rack de servere dintr-o cameră întunecată."],
    "Misiunea 14: Secrete de Familie"
)

add_mission(
    14, "Secrete de Familie", "Teddy", "300 EUR", "Misiunea 13: Încolțit",
    ["Întâlnește-te cu Teddy la cafenea.", "Ascultă vestea despre sarcina Magdei.", "Mergi la întâlnirea cu Căpitanu' la terasă.", "Respinge atacul ambuscadă al rivalilor."],
    "Teddy îl cheamă pe Relu disperat: Magda este însărcinată și vor să păstreze copilul. Relu este șocat, dar trebuie să meargă la Căpitanu' să-i spună vestea. În timpul întâlnirii lor tensionate de la o terasă din Pantelimon, o mașină a rivalilor trece prin zonă și deschide focul (Drive-by). Jucătorul trebuie să-i protejeze pe Căpitanu' și pe Teddy și să-i elimine pe atacatori.",
    "- Teddy: 'Relu... Magda e însărcinată. Te rog nu mă omorî.'\n- Căpitanu': 'Ce naiba zici acolo, mă?! Nepot de recuperator?!'\n- Relu: 'Atenție! La pământ! Trag ăștia!'",
    ["[Cadru 1] Teddy stă cu capul în mâini la o masă de plastic, în timp ce Relu îl privește cu o furie stăpânită.",
     "[Cadru 2] O mașină neagră cu geamuri fumurii trece în viteză, trăgând rafale de gloanțe spre terasă.",
     "[Cadru 3] Relu trage de după o masă răsturnată, protejându-l pe Teddy cu corpul său."],
    "Misiunea 15: Trădarea"
)

add_mission(
    15, "Trădarea", "Nea Puiu", "400 EUR", "Misiunea 14: Secrete de Familie",
    ["Urmărește-o pe Nico fără să fii detectat.", "Filmează întâlnirea ei secretă cu Emilian.", "Scapă din zona de întâlnire fără alertă."],
    "Nea Puiu îi spune lui Relu că Nico se întâlnește în secret cu un tip ciudat care pare a fi polițist (Emilian). Relu o urmărește pe Nico prin București până la o fabrică dezafectată din marginea orașului, unde o vede oferind informații inspectorului Emilian. Jucătorul trebuie să colecteze dovezi foto/video și să plece fără a fi detectat.",
    "- Nea Puiu: 'Fata aia, Nico... se învârte cu cine nu trebuie, Relule. O miroase a gabor de la o poștă.'\n- Nico: 'Ți-am dat ce doreai, Emilian. Acum lasă-mă în pace!'",
    ["[Cadru 1] Relu folosește zoom-ul unui aparat foto vechi de pe acoperișul unei clădiri ruinate.",
     "[Cadru 2] În vizor se văd Nico și Emilian discutând aprins lângă un morman de moloz.",
     "[Cadru 3] Relu coboară treptele de urgență în liniște, ascunzând aparatul foto sub haină."],
    "Misiunea 16: Punct și de la Capăt"
)

add_mission(
    16, "Punct și de la Capăt", "Toma", "1000 EUR, M4", "Misiunea 15: Trădarea",
    ["Mergi la întâlnirea secretă cu Toma la Constanța.", "Fă pactul secret pentru eliminarea Căpitanului.", "Întoarce-te în Pantelimon și apără-te de oamenii Căpitanului."],
    "Finalul Sezonului 1. Relu realizează că este încolțit de poliție și de propriul șef. Face o călătorie rapidă la Constanța pentru a se întâlni cu Toma, marele șef care controlează portul. Toma îi propune lui Relu să-l trădeze pe Căpitanu' în schimbul protecției și preluării afacerilor din sectorul 2. Relu acceptă pactul, dar la întoarcerea în București este atacat de oamenii Căpitanului care suspectează trădarea. O luptă masivă de supraviețuire la depozitul din Pantelimon.",
    "- Toma: 'Căpitanu' e de domeniul trecutului, Relu. E prea zgomotos și prost. Tu ești băiat deștept. Îl curățăm și preiei tu.'\n- Relu: 'Să moară Căpitanu'.'",
    ["[Cadru 1] Relu și Toma stau pe o terasă luxoasă din portul Tomis, privind spre iahturi.",
     "[Cadru 2] O explozie violentă distruge garajul lui Nea Puiu în București.",
     "[Cadru 3] Relu, plin de funingine și sânge, ține în mână o armă automată M4, privind spre flăcări."],
    "Misiunea 17: Nuntă cu Scântei"
)

# --- SEZONUL 2 ---
add_mission(
    17, "Nuntă cu Scântei", "Teddy", "500 EUR", "Misiunea 16: Punct și de la Capăt",
    ["Mergi la restaurantul unde are loc nunta lui Teddy cu Magda.", "Verifică invitații la intrare și confiscă armele.", "Oprește scandalul dintre interlopii din Constanța și cei din Pantelimon."],
    "Debutul Sezonului 2. Relația Magdei cu Teddy duce la o nuntă forțată pentru a stabili o alianță fragilă între Căpitanu' și facțiunea lui Toma (cu Relu ca intermediar). Relu trebuie să asigure securitatea la nuntă. Lucrurile degenerează când interlopii din Constanța și cei din Pantelimon se îmbată și scot cuțitele. Jucătorul trebuie să intervină fără a folosi arme de foc pentru a nu strica nunta fiicei sale.",
    "- Căpitanu': 'Băi Relule, cuscrii tăi de la Constanța cam strâmbă din nas la mâncarea noastră. Spune-le să se potolească!'\n- Relu: 'Nu strică nimeni nunta fetei mele. Cine mai scoate un cuțit, pleacă de aici cu picioarele înainte.'",
    ["[Cadru 1] Magda în rochie albă de mireasă plânge lângă un Teddy îmbrăcat în costum de ginere strâmt.",
     "[Cadru 2] O masă plină de aperitive este răsturnată în timp ce doi interlopi masivi se iau de gât.",
     "[Cadru 3] Relu îi dă un pumn în figură unui interlop din Constanța, trimițându-l direct în tortul de nuntă."],
    "Misiunea 18: Cadoul Căpitanului"
)

add_mission(
    18, "Cadoul Căpitanului", "Căpitanu'", "400 EUR, Uzi", "Misiunea 17: Nuntă cu Scântei",
    ["Mergi la casa primită cadou de tineri în sectorul 2.", "Elimină ocupanții ilegali trimiși de o bandă rivală.", "Asigură perimetrul pentru mutare."],
    "Căpitanu' le oferă tinerilor căsătoriți o casă naționalizată în sectorul 2, dar aceasta este ocupată abuziv de o bandă de recuperatori rivali asociați cu Nicu (interlopul întors din Spania). Relu și Teddy merg să 'elibereze' proprietatea cu forța, folosind bâte de baseball și arme ușoare.",
    "- Teddy: 'Tatăl meu ne-a dat casa asta, dar băieții ăștia zic că e a lor. Relu, ce facem?'\n- Relu: 'Ce știm mai bine. Scoate bâta.'",
    ["[Cadru 1] O vilă veche, dărăpănată, cu graffiti pe pereți, înconjurată de curte plină de gunoaie.",
     "[Cadru 2] Teddy lovește cu o bâtă de baseball ușa de la intrare, spărgând lemnul putred.",
     "[Cadru 3] Relu amenință un interlop plin de tatuaje care fuge disperat peste gard."],
    "Misiunea 19: Inspectorul Psihopat"
)

add_mission(
    19, "Inspectorul Psihopat", "Emilian", "300 EUR", "Misiunea 18: Cadoul Căpitanului",
    ["Răspunde la telefonul lui Emilian.", "Mergi la locul de întâlnire de sub podul Grant.", "Plantează pachetul de cocaină în mașina senatorului.", "Sună la 112 anonim."],
    "Inspectorul Emilian îl șantajează direct pe Relu cu dovezile crimei din sezonul 1. Pentru a-l lăsa în pace, Emilian îi ordonă lui Relu să compromită un senator curat care investighează corupția din poliție. Relu trebuie să se furișeze în parcarea privată a senatorului, să planteze o cantitate masivă de cocaină sub scaunul șoferului și apoi să sune la poliție dintr-o cabină telefonică publică.",
    "- Emilian: 'Relu, ești o umbră în buzunarul meu. Faci ce spun eu, sau soția ta află unde ai îngropat cadavrul ăla.'\n- Relu: 'Ce vrei să fac?'\n- Emilian: 'O mică surpriză pentru un domn senator...'",
    ["[Cadru 1] Emilian zâmbește maniacal sub lumina galbenă a unui stâlp de sub podul Grant.",
     "[Cadru 2] Relu sparge discret geamul unei limuzine negre folosind un dispozitiv special.",
     "[Cadru 3] Relu vorbește la un telefon public vechi, acoperindu-și gura cu o batistă."],
    "Misiunea 20: Spălare de Bani"
)

add_mission(
    20, "Spălare de Bani", "Nico", "800 EUR", "Misiunea 19: Inspectorul Psihopat",
    ["Mergi la spălătoria auto a clanului din Pantelimon.", "Colectează încasările fictive.", "Transportă banii la firma de amanet din Obor.", "Elimină hoții care încearcă să te jefuiască pe drum."],
    "Nico are nevoie de ajutorul lui Relu pentru a rula banii murdari proveniți din prostituție și contrabandă prin intermediul unei spălătorii auto și a unei case de amanet. În timpul transportului banilor, Relu este atacat în trafic de doi motocicliști înarmați trimiși de Nicu. Jucătorul trebuie să conducă defensiv, să-i elimine pe urmăritori și să predea banii în siguranță.",
    "- Nico: 'Banii ăștia trebuie spălați repede. Ai grijă, Nicu a aflat de traseu.'\n- Relu: 'Să încerce doar să se apropie de mașină.'",
    ["[Cadru 1] Relu numără teancuri de bancnote murdare într-un birou mic din spatele spălătoriei auto.",
     "[Cadru 2] O urmărire pe șoseaua Pantelimon: Relu lovește cu portiera Loganului un motociclist înarmat.",
     "[Cadru 3] Relu intră în casa de amanet cu o geantă sport neagră, lăsând în urmă o epavă de motocicletă arzând."],
    "Misiunea 21: Umbra lui Nea Puiu"
)

add_mission(
    21, "Umbra lui Nea Puiu", "Nea Puiu", "200 EUR", "Misiunea 20: Spălare de Bani",
    ["Mergi la garajul lui Nea Puiu.", "Găsește-l pe Nea Puiu care a plecat paranoic prin cartier.", "Salvează-l de gaborii care vor să-l legitimeze.", "Du-l la o casă conspirativă din afara Bucureștiului."],
    "Nea Puiu începe să sufere de episoade paranoice severe din cauza vârstei și a alcoolului. Acesta pleacă prin Pantelimon înarmat cu un cuțit, strigând că poliția este pe urmele lor. Relu trebuie să-l găsească rapid înainte de a face o prostie, să-l salveze de o patrulă de poliție locală pe care Nea Puiu o amenința și să-l transporte în siguranță la o casă conspirativă din județul Ilfov.",
    "- Nea Puiu: 'Vin după noi, Relule! Gaborul ăla cu ochi de sticlă știe tot! Trebuie să-i curățăm pe toți!'\n- Relu: 'Nea Puiu, potolește-te. Urcă în mașină, mergem la aer curat.'",
    ["[Cadru 1] Nea Puiu agită un cuțit ruginit în mijlocul unei intersecții din Pantelimon, speriind trecătorii.",
     "[Cadru 2] Relu intervine între doi polițiști locali și Nea Puiu, oferindu-le gaborilor o șpagă grasă.",
     "[Cadru 3] Nea Puiu doarme pe scaunul din dreapta al Loganului în timp ce mașina rulează pe un drum de țară."],
    "Misiunea 22: Transport de Constanța"
)

add_mission(
    22, "Transport de Constanța", "Toma", "1000 EUR, Sniper Rifle", "Misiunea 21: Umbra lui Nea Puiu",
    ["Mergi pe autostrada A2 (Autostrada Soarelui).", "Întâlnește camionul cu marfă de la Constanța.", "Apără transportul de oamenii lui Nicu.", "Escortează camionul la depozitul din București."],
    "Toma trimite un transport masiv de marfă de contrabandă (țigări și alcool) către București. Oamenii lui Nicu organizează un baraj rutier pe autostradă pentru a fura marfa. Relu, echipat cu o pușcă cu lunetă (Sniper Rifle) primită de la Toma, trebuie să urce pe un pod peste autostradă, să elimine atacatorii de la distanță și apoi să conducă camionul până în Pantelimon.",
    "- Toma: 'Transportul ăsta e testul tău, Relu. Dacă marfa nu ajunge, înțelegerea noastră pică.'\n- Relu: 'O să ajungă. Punct.'",
    ["[Cadru 1] Relu stă întins pe burtă pe un pod de ciment peste A2, privind prin luneta puștii.",
     "[Cadru 2] Explozii și focuri de armă pe autostradă: o mașină a atacatorilor explodează după ce Relu trage în rezervor.",
     "[Cadru 3] Camionul trece peste resturile barajului, condus de un Relu impasibil."],
    "Misiunea 23: Jocul Dublu al lui Nico"
)

add_mission(
    23, "Jocul Dublu al lui Nico", "Nico", "600 EUR", "Misiunea 22: Transport de Constanța",
    ["Întâlnește-te cu Nico la un hotel de tranzit.", "Află că Emilian vrea să o omoare.", "Obține pașaportul fals de la falsificatorul din Colentina.", "Predă pașaportul lui Nico și ajută-o să fugă."],
    "Nico este disperată: Emilian a realizat că ea a încercat să-l mintă și plănuiește să o elimine. Ea îi cere ajutorul lui Relu pentru a fugi din țară. Relu trebuie să meargă în cartierul Colentina la un falsificator de documente extrem de dubios, să recupereze un pașaport fals sub amenințarea pistolului și să i-l livreze lui Nico la un motel, ajutând-o să treacă nevăzută de oamenii lui Emilian.",
    "- Nico: 'Relu, Emilian e nebun! O să mă omoare! Trebuie să plec din țară acum!'\n- Falsificator: 'Băiatu', pașaportul ăsta costă dublu acum că e grabă.'\n- Relu (punându-i pistolul la tâmplă): 'Plătesc cu plumb dacă nu mi-l dai acum.'",
    ["[Cadru 1] Relu îl strânge de gât pe falsificator peste o masă plină de vopsele și prese de imprimat.",
     "[Cadru 2] Nico stă speriată la fereastra unui motel ieftin, privind spre parcare unde patrulează o mașină suspectă.",
     "[Cadru 3] Nico fuge pe ușa din spate a motelului, strângând pașaportul la piept."],
    "Misiunea 24: Răzbunarea lui Nicu"
)

add_mission(
    24, "Răzbunarea lui Nicu", "Căpitanu'", "800 EUR, Shotgun", "Misiunea 23: Jocul Dublu al lui Nico",
    ["Mergi de urgență la sala de box a lui Relu.", "Apără sala de asaltul oamenilor lui Nicu.", "Elimină toți atacatorii și securizează zona."],
    "Nicu lansează un atac direct asupra teritoriului lui Relu pentru a trimite un mesaj Căpitanului. Sala de box a lui Relu este asaltată de zeci de oameni înarmați. Jucătorul trebuie să apere sala folosind un arsenal variat (shotgun, pistol, grenade), transformând subsolul blocului într-un adevărat câmp de luptă.",
    "- Nicu (mesaj audio): 'Relule, ai crezut că ești șmecher cu Constanța ta? Îți dărâm tot cartierul, spaniolule!'\n- Relu: 'Ne vedem la sală, Nicu. Adu mulți oameni.'",
    ["[Cadru 1] Oamenii lui Nicu sparg ușile sălii de box aruncând cocteiluri Molotov.",
     "[Cadru 2] Relu trage cu un shotgun din spatele unui stâlp de beton, în timp ce gloanțele distrug oglinzile din sală.",
     "[Cadru 3] Cadavre de interlopi zac pe ringul de box acoperit de cioburi și fum."],
    "Misiunea 25: Capcana"
)

# --- SEZONUL 2 CONTINUARE ---
add_mission(
    25, "Capcana", "Emilian", "0 EUR (Misiune de Evadare)", "Misiunea 24: Răzbunarea lui Nicu",
    ["Mergi la întâlnirea aranjată de Emilian la un bloc turn.", "Realizează că este o capcană și că ești înconjurat de mascați.", "Evadează din clădire folosind acoperișurile și ghenele de gunoi."],
    "Emilian îl atrage pe Relu într-o capcană la un bloc turn din Pantelimon, sub pretextul unei noi sarcini. Când Relu ajunge, clădirea este înconjurată de trupele speciale ale poliției (mascați). Jucătorul trebuie să treacă prin apartamente, să urce pe acoperiș, să sară pe schelele exterioare de reabilitare a blocului și să evadeze prin ghena de gunoi pentru a scăpa de arest.",
    "- Emilian: 'Sfârșitul jocului, Relu! De data asta nu mai scapi!'\n- Relu: 'Niciodată să nu spui niciodată, gaborule.'",
    ["[Cadru 1] Mascați înarmați până în dinți urcă pe scările blocului, spărgând uși.",
     "[Cadru 2] Relu aleargă pe acoperișul din smoală al blocului gri, sub lumina reflectoarelor unui elicopter.",
     "[Cadru 3] Relu sare într-un container mare de gunoi din spatele blocului, scăpând la limită."],
    "Misiunea 26: Pactul"
)

add_mission(
    26, "Pactul", "Nicu", "1000 EUR", "Misiunea 25: Capcana",
    ["Mergi la întâlnirea secretă cu Nicu la un depozit de fier vechi.", "Negociază trădarea Căpitanului.", "Elimină bodyguarzii Căpitanului care te-au spionat."],
    "Relu se întâlnește cu Nicu la un depozit de fier vechi din Pantelimon. Nicu îi propune să-l ajute să-l elimine pe Căpitanu', promițând că familia lui Relu nu va fi atinsă. În timpul discuției, Relu observă doi dintre oamenii Căpitanului care îi spionau. Trebuie să-i urmărească și să-i elimine înainte de a raporta Căpitanului despre întâlnire.",
    "- Nicu: 'Hai să terminăm cu moșul ăsta de Căpitanu'. Tu îți vezi de treabă, eu preiau sectorul. Ce zici?'\n- Relu: 'Dacă se atinge cineva de familia mea, vă curăț pe toți. Accept pactul.'" ,
    ["[Cadru 1] Relu și Nicu discută înconjurați de munți de mașini strivite la fier vechi.",
     "[Cadru 2] O urmărire pe jos printre containere: Relu prinde din urmă un spion al Căpitanului.",
     "[Cadru 3] Relu îl strânge de gât pe spion cu o sârmă în spatele unui vagon de tren ruginit."],
    "Misiunea 27: Confruntarea Finală"
)

add_mission(
    27, "Confruntarea Finală", "Gina", "500 EUR, AK-47", "Misiunea 26: Pactul",
    ["Mergi de urgență la apartamentul lui Relu.", "Apără apartamentul de asaltul oamenilor lui Nicu (care au trădat pactul).", "Du-i pe Gina și Chuckie într-un loc sigur."],
    "Nicu trădează pactul și trimite o echipă de ucigași direct la apartamentul lui Relu pentru a-i elimina familia și a nu lăsa martori. Jucătorul ajunge chiar în momentul în care ușa este spartă. Trebuie să-și folosească arsenalul pentru a curăța apartamentul și scara blocului de atacatori, protejându-și familia îngrozită.",
    "- Gina: 'Relu! Trag în noi! Ajutor!'\n- Relu: 'Gina, Chuckie, sub pat! Acum!'\n- Relu (încărcând AK-47): 'V-ați luat de cine nu trebuie, gunoaielor!'" ,
    ["[Cadru 1] Ușa apartamentului lui Relu este spulberată de gloanțe, Gina țipând în bucătărie.",
     "[Cadru 2] Relu trage o rafală de AK-47 pe holul îngust al blocului, doborând doi atacatori.",
     "[Cadru 3] Relu îi urcă pe Gina și Chuckie speriați în Loganul plin de găuri de gloanțe."],
    "Misiunea 28: Sânge pe Zăpadă"
)

add_mission(
    28, "Sânge pe Zăpadă", "Toma", "1500 EUR, RPG", "Misiunea 27: Confruntarea Finală",
    ["Mergi la hangarul abandonat de lângă Lacul Pantelimon.", "Elimină-l pe Nicu și garda sa de corp.", "Evită atacul de sniper al oamenilor lui Emilian."],
    "Finalul Sezonului 2. Relu și Toma organizează asaltul final asupra hangarului unde se ascunde Nicu. Misiunea este un masacru pe zăpada din jurul lacului Pantelimon. Jucătorul folosește un RPG pentru a distruge mașinile de lux ale lui Nicu și îl execută pe Nicu. Totuși, în timpul luptei, Nea Puiu este ucis de un lunetist trimis de Emilian, iar Relu realizează că războiul abia a început.",
    "- Nicu: 'Relu, te rog... îți dau toți banii din Spania...'\n- Relu: 'Ai atins ușa casei mele. Să-i spui lui Nea Puiu că-mi pare rău.' (Trage)\n- Emilian (prin stație): 'O să plângi mult timp de acum încolo, Relu...'",
    ["[Cadru 1] Un hangar mare din tablă ruginită pe malul înghețat al Lacului Pantelimon, acoperit de zăpadă.",
     "[Cadru 2] Relu trage cu RPG-ul într-un SUV alb aparținând lui Nicu, creând o explozie uriașă.",
     "[Cadru 3] Relu privește corpul inert al lui Nea Puiu întins pe zăpadă, strângându-și pumnii de furie."],
    "Misiunea 29: Noua Ordine"
)

# --- SEZONUL 3 ---
add_mission(
    29, "Noua Ordine", "Toma", "1000 EUR", "Misiunea 28: Sânge pe Zăpadă",
    ["Mergi la terasa Căpitanului (care acum este condusă de Teddy).", "Organizează noii oameni din Pantelimon.", "Recuperează taxa de la cluburile din sectorul 2."],
    "Debutul Sezonului 3. Căpitanu' este scos din joc (paralizat sau mort în urma atacurilor), iar Teddy încearcă să preia conducerea clanului sub îndrumarea lui Relu. Jucătorul trebuie să viziteze 3 cluburi mari din sectorul 2 pentru a re-impune autoritatea noului clan, bătându-i pe patronii care cred că pot refuza plata acum că bătrânul a dispărut.",
    "- Teddy: 'Relu, unii patroni zic că nu mă respectă pe mine. Că sunt doar băiatul Căpitanului.'\n- Relu: 'O să ne respecte după ce le spargem barurile. Hai să le facem o vizită.'",
    ["[Cadru 1] Teddy stă pe scaunul de piele al Căpitanului în biroul terasei, arătând nesigur.",
     "[Cadru 2] Relu sparge o sticlă de whisky de capul unui patron de club care refuza taxa.",
     "[Cadru 3] Patronul speriat semnează teancul de chitanțe în timp ce Relu își șterge mâinile."],
    "Misiunea 30: Presiunea Gaborilor"
)

add_mission(
    30, "Presiunea Gaborilor", "Teddy", "800 EUR", "Misiunea 29: Noua Ordine",
    ["Mergi la sediul poliției din sectorul 2.", "Întâlnește-te cu polițistul corupt Sabău.", "Livrează geanta cu mită de 20.000 EUR.", "Elimină agenții secreți care monitorizează tranzacția."],
    "Emilian a pus presiune pe toate patrulele din cartier. Teddy și Relu trebuie să mituiască un comisar (Sabău) pentru a slăbi controalele. Relu livrează geanta într-o parcare subterană din Pantelimon, dar tranzacția este supravegheată de agenți de la afaceri interne. Jucătorul trebuie să elimine spionii înainte ca aceștia să raporteze.",
    "- Sabău: 'Emilian e nebun, băieți. Nu mai pot să-l țin în frâu mult timp. Riscul e mare, mă costă mai mult.'\n- Relu: 'Banii sunt aici. Ai grijă să nu vedem gabori pe stradă diseară.'",
    ["[Cadru 1] O parcare subterană slab iluminată, cu stâlpi de beton plini de igrasie.",
     "[Cadru 2] Relu îi predă comisarului Sabău o geantă sport neagră prin geamul mașinii.",
     "[Cadru 3] Relu elimină cu un pistol cu silențios doi agenți ascunși într-o mașină utilitară."],
    "Misiunea 31: Magazinul de Electrocasnice"
)

add_mission(
    31, "Magazinul de Electrocasnice", "Teddy", "600 EUR, Respect +30", "Misiunea 30: Presiunea Gaborilor",
    ["Deschide noul magazin de electrocasnice (front de spălare bani).", "Alungă recuperatorii din sectorul 3 care cer taxă de protecție.", "Intimidează-l pe liderul lor."],
    "Pentru a spăla banii lui Toma, Relu și Teddy deschid un magazin de electrocasnice în Pantelimon. Niște interlopi mărunți din sectorul 3, neștiind cine controlează magazinul, vin să ceară taxă de protecție. Jucătorul trebuie să apere magazinul folosind un frigider sau televizoare vechi pentru a-i bate pe atacatori, apoi să-l captureze pe liderul lor și să-l lege de un stâlp.",
    "- Teddy: 'Vin băieții din sectorul 3 să ne ceară taxă. Pe teritoriul nostru!'\n- Relu: 'Hai să le arătăm niște oferte speciale la mașinile de spălat.'",
    ["[Cadru 1] Trei interlopi sparg vitrina magazinului de electrocasnice cu bâte.",
     "[Cadru 2] Relu împinge un frigider mare peste un atacator, strivindu-l de perete.",
     "[Cadru 3] Liderul recuperatorilor este legat cu bandă adezivă de un stâlp de înaltă tensiune în fața magazinului."],
    "Misiunea 32: Dispariția lui Nico"
)

add_mission(
    32, "Dispariția lui Nico", "Nico", "800 EUR", "Misiunea 31: Magazinul de Electrocasnice",
    ["Mergi la ultimul semnal al telefonului lui Nico pe DN1.", "Infiltrează-te în motelul suspect.", "Elimină gărzile de corp ale lui Emilian.", "Eliberează-o pe Nico din subsol."],
    "Nico a fost capturată de Emilian și închisă într-un motel pe DN1 pentru a fi interogată și torturată. Relu primește un indiciu despre locație. Trebuie să se infiltreze în motel, să elimine oamenii lui Emilian și să o salveze pe Nico, care este într-o stare fizică critică în subsolul clădirii.",
    "- Nico (mesaj slab): 'Relu... ajutor... e subsolul de la...'\n- Relu: 'Rezistă, Nico. Vin acum.'",
    ["[Cadru 1] Relu taie curentul electric al motelului de la panoul exterior.",
     "[Cadru 2] Relu curăță holurile motelului folosind un pistol cu amortizor sub lumina roșie de urgență.",
     "[Cadru 3] Relu o găsește pe Nico legată de un scaun în subsolul inundat, tăindu-i sforile."],
    "Misiunea 33: Avertismentul lui Toma"
)

add_mission(
    33, "Avertismentul lui Toma", "Toma", "1200 EUR", "Misiunea 32: Dispariția lui Nico",
    ["Mergi la întâlnirea cu oamenii lui Toma în portul Constanța.", "Află că Toma este nemulțumit de pierderile provocate de Emilian.", "Elimină dealerii concurenți din stațiunea Mamaia."],
    "Toma este furios că acțiunile lui Emilian blochează portul și afacerile cu droguri. Îl trimite pe Relu să 'facă curățenie' printre dealerii independenți din Mamaia care atrag atenția poliției. Jucătorul trebuie să elimine 4 ținte diferite în stațiune într-un timp limită, folosind o barcă de mare viteză pentru a fugi.",
    "- Toma: 'Bucureștiul tău îmi aduce numai probleme, Relu. Rezolvă dealerii ăia sau vin eu personal și curăț tot, inclusiv pe tine.'\n- Relu: 'Consideră-i rezolvați.'",
    ["[Cadru 1] Relu conduce o barcă cu motor pe valurile mării negre sub lumina apusului.",
     "[Cadru 2] Relu trage de pe barcă într-un dealer care încearcă să fugă pe plaja din Mamaia.",
     "[Cadru 3] Barca se îndepărtează în viteză în timp ce pe mal se aud sirenele poliției."],
    "Misiunea 34: Trădătorul"
)

add_mission(
    34, "Trădătorul", "Teddy", "700 EUR", "Misiunea 33: Avertismentul lui Toma",
    ["Identifică trădătorul din interiorul clanului.", "Urmărește-l pe trădător prin cartier.", "Răpește-l și du-l în subsolul unui bloc.", "Interroghează-l pentru a afla ce i-a spus lui Emilian."],
    "Teddy descoperă că unul dintre băieții vechi ai Căpitanului oferă informații direct lui Emilian. Jucătorul trebuie să-l identifice la o bodegă din Pantelimon, să-l urmărească discret și să-l răpească introducându-l în portbagajul Daciei Logan. Misiunea se termină cu o scenă interactivă de interogatoriu în subsolul blocului unde Relu folosește un clește și un baros pentru a scoate adevărul.",
    "- Trădător: 'Nu i-am spus nimic important, jur! Emilian m-a bătut, n-am avut de ales!'\n- Relu: 'Alegerea o faci acum: îmi spui tot sau nu mai pleci de aici pe picioare.'",
    ["[Cadru 1] Relu îl urmărește pe trădător mergând pe o alee întunecată printre blocuri.",
     "[Cadru 2] Relu îl lovește pe trădător în stomac, băgându-l cu forța în portbagajul taxiului galben.",
     "[Cadru 3] În subsol, sub o singură lumină chioară, Relu ține un baros deasupra genunchilor trădătorului."],
    "Misiunea 35: O Vizită la Constanța"
)

# --- SEZONUL 3 CONTINUARE ---
add_mission(
    35, "O Vizită la Constanța", "Toma", "1500 EUR, Combat Sniper", "Misiunea 34: Trădătorul",
    ["Mergi la Constanța la vila lui Toma.", "Acceptă misiunea de asasinat asupra șefului vămii portuare.", "Infiltrează-te în zona restricționată a portului.", "Elimină ținta cu o armă cu lunetă de pe o macara."],
    "Toma dorește eliminarea șefului vămii din portul Constanța, care a refuzat șpaga și blochează transporturile. Relu merge la Constanța, se infiltrează în zona industrială a portului noaptea, urcă pe o macara gigantică și execută ținta de la distanță mare folosind o pușcă cu lunetă avansată (Combat Sniper).",
    "- Toma: 'Vameșul ăsta crede că e cinstit. Arată-i că cinstea se plătește cu viața în portul meu.'\n- Relu: 'Sunt pe macara. Ținta e în vizor. Tragi-mi aer în piept. (Foc)'",
    ["[Cadru 1] Relu urcă treptele de fier ale unei macarale portuare uriașe, sub ploaia măruntă.",
     "[Cadru 2] Vizorul lunetei încadrează capul vameșului care discută la telefon în biroul său cu geamuri mari.",
     "[Cadru 3] Geamul se sparge în mii de bucăți în timp ce vameșul cade peste birou, ucis pe loc."],
    "Misiunea 36: Întoarcerea Acasă"
)

add_mission(
    36, "Întoarcerea Acasă", "Relu", "0 EUR (Misiune de Supraviețuire)", "Misiunea 35: O Vizită la Constanța",
    ["Condu pe drumul de întoarcere spre București.", "Supraviețuiește ambuscadei de pe DN3A.", "Luptă-te cu mercenarii prin lanul de porumb.", "Găsește un vehicul funcțional și întoarce-te în Pantelimon."],
    "Pe drumul de întoarcere de la Constanța, Dacia lui Relu este lovită intenționat de un tir și aruncată în decor pe DN3A. Relu, rănit, trebuie să iasă din mașină și să se apere de o echipă de mercenari trimiși de Toma (care a decis să se debaraseze de Relu după finalizarea hit-ului). Misiunea implică o luptă intensă de tip stealth/survival printr-un lan mare de porumb, folosind doar un cuțit și armele recuperate de la inamici.",
    "- Mercenar: 'Găsiți-l! E rănit, nu poate fi departe! Toma vrea capul lui!'\n- Relu (în șoaptă, în porumb): 'Toma... ai făcut cea mai mare greșeală din viața ta.'" ,
    ["[Cadru 1] Dacia Logan galbenă este răsturnată într-un șanț adânc, cu motorul fumegând.",
     "[Cadru 2] Relu stă ascuns în lanul înalt de porumb, strângând un cuțit militar plin de sânge.",
     "[Cadru 3] Relu conduce un tractor vechi furat de la o fermă din apropiere, îndreptându-se spre București."],
    "Misiunea 37: Obsesia lui Emilian"
)

add_mission(
    37, "Obsesia lui Emilian", "Gina", "500 EUR", "Misiunea 36: Întoarcerea Acasă",
    ["Răspunde la apelul plin de panică al Ginei.", "Mergi la fosta fabrică de sticlă din Pantelimon.", "Elimină capcanele explozive ale lui Emilian.", "Salvează-l pe Chuckie din mâinile lui Emilian."],
    "Emilian, complet obsedat și paranoic, l-a răpit pe Chuckie pentru a-l forța pe Relu să vină la o confruntare finală. Relu merge la fabrica abandonată de sticlă din Pantelimon. Locația este plină de capcane cu fir (tripwires) și explozibili. Jucătorul trebuie să dezactiveze capcanele, să elimine ultimii gabori fideli lui Emilian și să-l salveze pe Chuckie înainte ca o bombă cu ceas să explodeze.",
    "- Gina: 'Relu! Emilian l-a luat pe Chuckie din fața blocului! A zis că dacă nu vii la fabrica de sticlă, îl omoară!'\n- Emilian (prin difuzor): 'Timpul trece, Relu! Ai 5 minute să-ți salvezi băiatul!'",
    ["[Cadru 1] Chuckie este legat de un stâlp de metal în mijlocul unei hale pline de cioburi de sticlă, cu un cronometru roșu clipind lângă el.",
     "[Cadru 2] Relu taie cu grijă firul unei grenade montate la intrarea în hală.",
     "[Cadru 3] Relu îl strânge în brațe pe Chuckie speriat, în timp ce în fundal se văd resturile capcanei dezactivate."],
    "Misiunea 38: Ochi pentru Ochi"
)

add_mission(
    38, "Ochi pentru Ochi", "Relu", "1000 EUR", "Misiunea 37: Obsesia lui Emilian",
    ["Urmărește-l pe Emilian care încearcă să fugă cu o mașină de poliție.", "Provoacă un accident mașinii sale pe Șoseaua Pantelimon.", "Execută-l pe Emilian într-o confruntare directă."],
    "Emilian fuge când vede că planul lui a eșuat. Urmează o urmărire intensă cu mașini pe Șoseaua Pantelimon. Jucătorul trebuie să lovească mașina lui Emilian până când aceasta se izbește de un stâlp de tramvai. Emilian coboară rănit și trage în Relu. Misiunea se încheie cu executarea lui Emilian de către Relu în mijlocul străzii, sub privirile trecătorilor îngroziți.",
    "- Emilian (plângând și râzând): 'Nu mă poți ucide, Relu... sunt legea...'\n- Relu: 'Aici, în Pantelimon, legea o scriu eu cu barosul.' (Trage un glonț în cap)",
    ["[Cadru 1] O mașină de poliție Logan albastră derapează violent pe linia de tramvai de pe Șoseaua Pantelimon.",
     "[Cadru 2] Emilian stă sprijinit de stâlpul de beton al tramvaiului, cu fața plină de sânge, trăgând haotic.",
     "[Cadru 3] Relu stă în picioare deasupra lui Emilian, cu arma îndreptată spre el, sub cerul gri al Bucureștiului."],
    "Misiunea 39: Tăierea Legăturilor"
)

add_mission(
    39, "Tăierea Legăturilor", "Relu", "500 EUR", "Misiunea 38: Ochi pentru Ochi",
    ["Mergi la Otopeni Airport.", "Escortează-i pe Gina, Magda, Teddy și Chuckie.", "Respinge asaltul trupelor lui Toma la terminalul plecări.", "Asigură-te că familia se îmbarcă în avionul spre Spania."],
    "Relu realizează că Toma nu se va opri până nu-l va vedea mort. Decide să-și trimită întreaga familie (Gina, copiii și Teddy) în Spania pentru a-i proteja. La aeroportul Otopeni, terminalul este atacat de o echipă masivă de asasini trimiși de Toma. Relu trebuie să reziste atacului și să acopere retragerea familiei sale până când aceștia trec de porțile de securitate.",
    "- Gina: 'Relu, tu nu vii cu noi?'\n- Relu: 'Mai am o treabă de terminat la Constanța, Gina. Aveți grijă de voi în Spania. O să vin și eu.'",
    ["[Cadru 1] Terminalul de plecări al Aeroportului Otopeni, plin de pasageri care fug panicați.",
     "[Cadru 2] Relu trage de după un ghișeu de check-in spre trei atacatori îmbrăcați în costume negre.",
     "[Cadru 3] Avionul decolează pe pistă în timp ce Relu privește prin geamul mare al terminalului, singur."],
    "Misiunea 40: Prăbușirea Imperiului"
)

add_mission(
    40, "Prăbușirea Imperiului", "Nico", "1000 EUR, RPG, M4", "Misiunea 39: Tăierea Legăturilor",
    ["Mergi la depozitul principal din Pantelimon.", "Apără depozitul de asaltul total al mafiei din Constanța.", "Elimină blindatele trimise de Toma."],
    "Toma lansează un asalt total asupra ultimului punct de rezistență din București—depozitul principal de la Granitul administrat de Nico. Nico și Relu, baricadați în depozit, trebuie să înfrunte valuri de inamici înarmați cu arme grele și vehicule blindate. O misiune de supraviețuire și luptă militară urbană la scară largă în stil GTA.",
    "- Nico: 'Relu, vin peste noi cu tot ce au! Dacă pică depozitul ăsta, am terminat-o!'\n- Relu: 'Pregătește lansatorul de rachete. O să fie măcel.'",
    ["[Cadru 1] Depozitul este înconjurat de mașini de teren negre care trag cu mitraliere grele.",
     "[Cadru 2] Relu folosește un RPG pentru a arunca în aer un SUV blindat la porțile depozitului.",
     "[Cadru 3] Nico trage cu un M4 de pe acoperișul depozitului, acoperită de explozii și fum."],
    "Misiunea 41: Răfuiala de la Constanța"
)

add_mission(
    41, "Răfuiala de la Constanța", "Relu", "2000 EUR, Minigun", "Misiunea 40: Prăbușirea Imperiului",
    ["Mergi la Constanța.", "Asaltează vila luxoasă a lui Toma pe plaja din Năvodari.", "Elimină armata privată a lui Toma.", "Execută-l pe Toma în biroul său."],
    "Relu merge la Constanța pentru a pune capăt amenințării o dată pentru totdeauna. Înzestrat cu un minigun și un arsenal complet, Relu asaltează vila masivă a lui Toma de pe plajă. Trebuie să lupte prin grădină, piscină, holuri și să ajungă în biroul de la etaj unde Toma stă baricadat. Relu îl confruntă pe Toma și îl execută fără milă.",
    "- Toma: 'Relu... nu poți distruge ce am construit eu... sunt prea mulți oameni în spatele meu...'\n- Relu: 'Oamenii tăi sunt morți în grădină, Toma. Acum e rândul tău.' (Trage o rafală de minigun)",
    ["[Cadru 1] O vilă albă modernă pe plaja din Năvodari, cu palmieri și o piscină mare plină de cadavre.",
     "[Cadru 2] Relu trage cu un Minigun devastator, distrugând stâlpii de marmură ai vilei.",
     "[Cadru 3] Toma stă prăbușit în scaunul său de piele roșie din birou, cu Relu privind spre mare prin geamul spart."],
    "Misiunea 42: Umbre în Pantelimon"
)

add_mission(
    42, "Umbre în Pantelimon", "Niciunul (Misiune Finală)", "Respect Maxim, Licență de Taxi de Aur", "Misiunea 41: Răfuiala de la Constanța",
    ["Mergi în parcarea de la Cora Pantelimon.", "Urcă în Loganul galben.", "Așteaptă ultimul pasager."],
    "Misiunea finală a jocului. Relu se întoarce în Pantelimon. Totul este liniștit acum, dar el este complet singur—familia lui este în siguranță în Spania, șefii și inamicii sunt toți morți. Relu își pornește taxiul Logan și parchează la Cora Pantelimon, privind apusul peste blocuri. O persoană misterioasă urcă pe bancheta din spate și îi cere să meargă spre o destinație secretă. Relu zâmbește amar, bagă în viteză și pornește mașina în timp ce genericul de final începe să ruleze pe o piesă de hip-hop românesc (B.U.G. Mafia - Pantelimonu' petrece). Fades to black.",
    "- Pasager: 'Salut, șefu'. Mergem până în Ferentari?'\n- Relu (privind în oglinda retrovizoare): 'Mergem oriunde vrei tu, prietene. Avem timp.'",
    ["[Cadru 1] Loganul galben stă singur sub cerul roșiatic al apusului în parcarea Cora Pantelimon.",
     "[Cadru 2] O mână deschide portiera din spate, lăsând să se vadă doar pantofi eleganți de piele.",
     "[Cadru 3] Mașina pleacă spre șoseaua Pantelimon, pierzându-se printre blocurile gri în timp ce genericul rulează."],
    "Sfârșitul Jocului (Joc Finalizat)"
)

def generate_missions():
    os.makedirs(MISSIONS_DIR, exist_ok=True)
    
    # State machine dict to build state_machine.md
    state_machine_data = []

    for m in MISSIONS:
        file_path = os.path.join(MISSIONS_DIR, f"mission_{m['num']:02d}.md")
        
        # Determine season based on mission number
        if m['num'] <= 16:
            season = "Sezonul 1"
        elif m['num'] <= 28:
            season = "Sezonul 2"
        else:
            season = "Sezonul 3"
            
        content = f"""# Misiunea {m['num']:02d}: {m['title']}
**Sezon / Episod corelat**: {season} (GTA Vice City Style)
**Client / Giver**: {m['giver']}
**Recompense**: {m['rewards']}
**Prerechizite (Prerequisites)**: {m['prereq']}

---

## Rezumat și Obiective
**Obiectiv Principal**: {m['desc']}

### Obiective de Gameplay:
"""
        for obj in m['obj']:
            content += f"- [ ] {obj}\n"
            
        content += f"""
---

## Dialoguri în Română (Slang de Pantelimon)
```dialogue
{m['dialogue']}
```

---

## Storyboard (Cadre Manga/Hentai - Prompturi Vizuale)
Acest storyboard conține descrierea cadrelor vizuale care ilustrează acțiunea misiunii pentru a ghida artistul grafic:

"""
        for panel in m['storyboard']:
            content += f"- **{panel}**\n"
            
        content += f"""
---

## Mașina de Stări a Jocului (Game State Machine)
- **Stare curentă**: `MISSION_{m['num']:02d}_ACTIVE`
- **Condiție de deblocare**: `{m['prereq']}` finalizată.
- **Tranziție**: La finalizarea tuturor obiectivelor, starea devine `MISSION_{m['num']:02d}_COMPLETED`.
- **Următoarea Misiune Deblocată**: `{m['next']}`.
"""
        with open(file_path, "w", encoding="utf-8") as f:
            f.write(content.strip() + "\n")
        print(f"Generated {file_path}")
        
        state_machine_data.append({
            "id": f"MISSION_{m['num']:02d}",
            "title": m['title'],
            "prereq": m['prereq'],
            "next": m['next']
        })
        
    # Generate state_machine.md
    sm_path = os.path.join(DOCS_DIR, "state_machine.md")
    sm_content = """# Game State Machine - GTA Vice City: Pantelimon (Umbre Storyline)

Această pagină descrie fluxul de tranziție al misiunilor și mașina de stări a jocului. Structura este un graf direcționat aciclic (DAG) împărțit pe 3 sezoane, corespunzător episoadelor serialului HBO Umbre.

## Diagramă de Flux (Workflow)

```mermaid
graph TD
"""
    for m in state_machine_data:
        # Simplify names for diagram
        node_id = m['id']
        node_title = m['title'].replace("'", "").replace('"', "")
        sm_content += f"    {node_id}[\"{node_id}: {node_title}\"]\n"
        
    sm_content += "\n"
    # Links dynamically built from prerequisites
    import re
    for m in state_machine_data:
        curr_id = m['id']
        prereq_str = m['prereq']
        matches = re.findall(r"Misiunea\s+(\d+)", prereq_str)
        for match in matches:
            parent_num = int(match)
            parent_id = f"MISSION_{parent_num:02d}"
            sm_content += f"    {parent_id} --> {curr_id}\n"
        
    sm_content += """```
 
 ## Stări Globale ale Jocului
 1. `STATE_NOT_STARTED`: Jucătorul nu a inițiat nicio misiune. Doar Free Roam în Pantelimon în taxi Logan.
 2. `STATE_MISSION_ACTIVE`: O misiune este în desfășurare. Obiectivele sunt afișate pe ecran (HUD). Salvarea jocului este dezactivată.
 3. `STATE_MISSION_FAILED`: Misiunea a eșuat (moartea protagonistului, distrugerea Loganului galben, eșecul obiectivelor). Respawn la Spitalul Sf. Pantelimon.
 4. `STATE_MISSION_SUCCESS`: Misiunea s-a încheiat cu succes. Se acordă bani, respect și se deblochează următoarea misiune în graf.
 5. `STATE_GAME_COMPLETED`: Toate cele 42 de misiuni au fost finalizate. Modul Free Roam este complet deblocat cu recompense speciale (Loganul de Aur).
 """
    with open(sm_path, "w", encoding="utf-8") as f:
        f.write(sm_content.strip() + "\n")
    print(f"Generated {sm_path}")

def generate_characters():
    os.makedirs(CHARACTERS_DIR, exist_ok=True)
    for char_id, info in CHARACTERS.items():
        file_path = os.path.join(CHARACTERS_DIR, f"{char_id}.md")
        content = f"""# {info['name']}
*{info['role']}*

---

## Descriere Generală
{info['description']}

---

## Biografie Wikipedia Style
{info['biography']}

---

## Relații și Afiliere
{info['relationships']}

---

## Trivia și Chestii Specifice (Romanian Easter Eggs)
{info['trivia']}
"""
        with open(file_path, "w", encoding="utf-8") as f:
            f.write(content.strip() + "\n")
        print(f"Generated {file_path}")

def generate_status():
    print("\n=== NARRATIVE WORKSPACE STATUS ===")
    missing_missions = []
    draft_missions = []
    critique_pending = []
    completed_missions = []
    
    for m in MISSIONS:
        m_num = m['num']
        m_id = f"MISSION_{m_num:02d}"
        file_path = os.path.join(MISSIONS_DIR, f"mission_{m_num:02d}.md")
        critique_path = os.path.join(MISSIONS_DIR, f"mission_{m_num:02d}.critique.md")
        
        if not os.path.exists(file_path):
            missing_missions.append(m_id)
        else:
            # Check structure
            with open(file_path, "r", encoding="utf-8") as f:
                content = f.read()
            has_obj = "Rezumat și Obiective" in content
            has_dial = "Dialoguri în Română" in content
            has_story = "Storyboard" in content
            has_state = "Mașina de Stări a Jocului" in content
            
            if os.path.exists(critique_path):
                critique_pending.append(m_id)
            elif not (has_obj and has_dial and has_story and has_state):
                draft_missions.append(m_id)
            else:
                completed_missions.append(m_id)
                
    print(f"Total Missions: {len(MISSIONS)}")
    print(f"  Completed: {len(completed_missions)}  ({len(completed_missions)}/42)")
    if draft_missions:
        print(f"  Draft/Incomplete: {len(draft_missions)}  {', '.join(draft_missions)}")
    if critique_pending:
        print(f"  Critique Pending: {len(critique_pending)}  {', '.join(critique_pending)}")
    if missing_missions:
        print(f"  Missing: {len(missing_missions)}  {', '.join(missing_missions)}")
    
    missing_chars = []
    completed_chars = []
    for char_id, info in CHARACTERS.items():
        file_path = os.path.join(CHARACTERS_DIR, f"{char_id}.md")
        if not os.path.exists(file_path):
            missing_chars.append(info['name'])
        else:
            completed_chars.append(info['name'])
            
    print(f"\nTotal Characters: {len(CHARACTERS)}")
    print(f"  Completed: {len(completed_chars)}  ({len(completed_chars)}/12)")
    if missing_chars:
        print(f"  Missing: {len(missing_chars)}  {', '.join(missing_chars)}")
    print("==================================\n")

def compile_context(mission_num):
    # Find target mission
    target_m = None
    for m in MISSIONS:
        if m['num'] == mission_num:
            target_m = m
            break
    if not target_m:
        print(f"Error: Mission {mission_num} not found.")
        sys.exit(1)
        
    context_path = os.path.join(MISSIONS_DIR, f"mission_{mission_num:02d}.context.md")
    
    # Identify relevant characters
    relevant_chars = []
    search_text = (target_m['title'] + " " + target_m['desc'] + " " + 
                   target_m['dialogue'] + " " + " ".join(target_m['storyboard'])).lower()
    
    for char_id, info in CHARACTERS.items():
        first_name = info['name'].split()[0].lower()
        if first_name in search_text or info['name'].lower() in search_text:
            relevant_chars.append(info)
            
    # Parse prerequisite details
    prereq_str = target_m['prereq']
    import re
    parent_nums = [int(n) for n in re.findall(r"Misiunea\s+(\d+)", prereq_str)]
    prereq_details = []
    for p_num in parent_nums:
        for m in MISSIONS:
            if m['num'] == p_num:
                prereq_details.append(m)
                
    # Build context markdown content
    content = f"# Focused Context for Mission {mission_num:02d}: {target_m['title']}\n"
    content += "Use this context to draft the mission markdown file. Keep the tone authentic to HBO's Umbre series and use Pantelimon landmarks.\n\n"
    
    content += "## 1. Parent/Prerequisite Missions Status\n"
    if prereq_details:
        for p in prereq_details:
            content += f"### Mission {p['num']:02d}: {p['title']}\n"
            content += f"**Description**: {p['desc']}\n"
            content += f"**Key Objectives achieved**:\n"
            for obj in p['obj']:
                content += f"- [x] {obj}\n"
            content += "\n"
    else:
        content += "No prerequisite missions (this is the starting mission).\n\n"
        
    content += "## 2. Relevant Character Biographies\n"
    if relevant_chars:
        for rc in relevant_chars:
            content += f"### {rc['name']} ({rc['role']})\n"
            content += f"**Description**: {rc['description']}\n"
            content += f"**Trivia**: {rc['trivia'].replace(chr(10), ' ')}\n\n"
    else:
        content += "No specific major characters detected in mission overview.\n\n"
        
    content += "## 3. Mission Objectives to Implement\n"
    for obj in target_m['obj']:
        content += f"- [ ] {obj}\n"
    content += "\n"
    
    content += "## 4. Expected Dialogues (Guideline)\n"
    content += f"```slang\n{target_m['dialogue']}\n```\n\n"
    
    content += "## 5. Storyboard Guidance\n"
    for frame in target_m['storyboard']:
        content += f"- {frame}\n"
        
    os.makedirs(MISSIONS_DIR, exist_ok=True)
    with open(context_path, "w", encoding="utf-8") as f:
        f.write(content.strip() + "\n")
    print(f"Context compiled successfully at: {context_path}")

def run_critique(mission_num):
    file_path = os.path.join(MISSIONS_DIR, f"mission_{mission_num:02d}.md")
    critique_path = os.path.join(MISSIONS_DIR, f"mission_{mission_num:02d}.critique.md")
    
    if not os.path.exists(file_path):
        print(f"Error: Mission file {file_path} does not exist.")
        sys.exit(1)
        
    with open(file_path, "r", encoding="utf-8") as f:
        content = f.read()
        
    errors = []
    warnings = []
    
    # 1. Structure check
    expected_headings = [
        "Rezumat și Obiective",
        "Dialoguri în Română",
        "Storyboard",
        "Mașina de Stări a Jocului"
    ]
    for h in expected_headings:
        if h not in content:
            errors.append(f"Missing required heading/section: '{h}'")
            
    # 2. Slang keyword check
    slang_keywords = ["gabor", "șmecher", "barosan", "logan", "mici", "afaceri", "recuperare", "bătaie", "ban", "pumn", "puiu", "căpitan"]
    found_keywords = [kw for kw in slang_keywords if kw in content.lower()]
    if len(found_keywords) < 2:
        warnings.append(f"Low usage of authentic Pantelimon slang (found only: {found_keywords}). Recommend adding local jargon like 'gabori', 'mici', or 'barosane'.")
        
    # 3. Storyboard layout check
    if "## Storyboard" in content:
        story_part = content.split("## Storyboard")[1].split("##")[0]
        if "- **[Cadru" not in story_part and "- **" not in story_part:
            errors.append("Storyboard panels do not follow the expected '- **[Cadru X] ...' or '- **...' list format.")
            
    # Write report
    if errors or warnings:
        critique_content = f"# Critique Report for Mission {mission_num:02d}\n\n"
        if errors:
            critique_content += "## ❌ Blockers (Must Fix)\n"
            for err in errors:
                critique_content += f"- {err}\n"
            critique_content += "\n"
        if warnings:
            critique_content += "## ⚠️ Suggestions (Highly Recommended)\n"
            for warn in warnings:
                critique_content += f"- {warn}\n"
            critique_content += "\n"
            
        with open(critique_path, "w", encoding="utf-8") as f:
            f.write(critique_content.strip() + "\n")
            
        print(f"Critique finished with {len(errors)} errors and {len(warnings)} warnings.")
        print(f"Report written to: {critique_path}")
        if errors:
            sys.exit(1)
    else:
        if os.path.exists(critique_path):
            os.remove(critique_path)
        print(f"CRITIQUE PASSED: Mission {mission_num:02d} matches all style and structure rules perfectly!")

if __name__ == "__main__":
    if len(sys.argv) > 1:
        cmd = sys.argv[1]
        if cmd == "generate":
            generate_missions()
            generate_characters()
        elif cmd == "status":
            generate_status()
        elif cmd == "context" and len(sys.argv) > 2:
            try:
                m_num = int(sys.argv[2])
                compile_context(m_num)
            except ValueError:
                print("Error: Mission number must be an integer.")
                sys.exit(1)
        elif cmd == "critique" and len(sys.argv) > 2:
            try:
                m_num = int(sys.argv[2])
                run_critique(m_num)
            except ValueError:
                print("Error: Mission number must be an integer.")
                sys.exit(1)
        elif cmd == "verify":
            sys.path.append(os.path.dirname(os.path.abspath(__file__)))
            import verify_storyline
            verify_storyline.verify_all()
        else:
            print(f"Unknown command: {cmd}")
            print("Usage: python harness.py [generate|status|context <num>|critique <num>|verify]")
    else:
        generate_missions()
        generate_characters()
