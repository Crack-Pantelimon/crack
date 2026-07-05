# Misiunea 02: Aparatele lui Mario
**Sezon / Episod corelat**: Sezonul 1 (GTA Vice City Style)
**Client / Giver**: Alex / Andrei (telefon)
**Recompense**: Variabilă – 100 RON dacă câștigi la păcănele, 0 RON dacă pierzi; Respect +1 ascuns dacă dai o țigară boschetarului.
**Prerechizite (Prerequisites)**: Misiune 1: Taximetria pe GPL

---

## Rezumat și Obiective
**Obiectiv Principal**: Ionel este atacat pe stradă de un random și se îndreaptă spre casă, unde mamaie apare întâmplător și îl avertizează că acei tipi sunt periculoși. Pe drum, primește un telefon de la Alex/Andrei, care îl invită la aparatele lui Mario. Misiunea pornește cu **25 lei** și poate duce la două trasee principale la păcănele: obține **100 lei** sau rămâi cu **0 lei**. Apoi trebuie să-și cumpere un pachet de țigări și un energizant pentru munca de a doua zi. În drum spre chioșc, are oportunitatea de a-i da o țigară unui boschetar pentru respect ascuns.

### Obiective de Gameplay:
- [ ] Ajungi la aparate și Mario face cinste cu 25 lei.
- [ ] Alege dacă accepți invitația la păcănele.
- [ ] Joacă păcănele și obține fie 100 lei, fie 0 lei.
- [ ] Cumpără un pachet de țigări și un energizant dacă mai ai bani.
- [ ] Dă o țigară boschetarului pentru respect ascuns și pierderea de 1 leu.

---

## Dialoguri în Română (Slang de Pantelimon)
```dialogue
- Mamaie: „Băiete, ăștia-s periculoși. Fugi și nu te întoarce, că ți-și vor face rău.”
- Alex/Andrei (la telefon): „Ce faci, bă, hai la aparate, că face Mario cinste. Bine, ajung în 10.”
- Mario (dacă pierzi): „Coaie, da-mi aia 25 de lei de ți i-am dat mai devreme, ce plm faci?”
- Mario (dacă obții 100 lei): „Oooo, fratelemeu, da și mie 50 de lei că de la mine ai jucat.”
- Ionel (dă bani): „Bine, frate, ia banii.”
- Ionel (nu dă): „Nu dau, stai cu banii tăi.”
- Boschetar (la ieșire): „Imi da o țigară, frate?”
- Ionel: „Ți-o dau, du-te și trage aer curat.”
```

---

## Storyboard ( Prompturi Vizuale)
Acest storyboard conține descrierea cadrelor vizuale care ilustrează acțiunea misiunii pentru a ghida artistul grafic:

- **[Cadru 1] Protagonistul este lovit pe stradă de un random și se retrage spre blocuri.**
- **[Cadru 2] Mamaie îi zice să fugă pentru că acei tipi sunt periculoși.**
- **[Cadru 3] Protagonistul vorbește la telefon cu Alex/Andrei înainte de a intra la barul lui Mario.**
- **[Cadru 4] Mario stă la aparatele de păcănele, zâmbind șmecherește și spunând ceva despre banii jucați.**
- **[Cadru 5] Protagonistul cumpără țigări și un energizant, iar boschetarul îi cere o țigară în fața chioșcului.**

---

## Mașina de Stări a Jocului (Game State Machine)
- **Stare curentă**: `MISSION_02_ACTIVE`
- **Condiție de deblocare**: `Misiunea 1: Taximetria pe GPL` finalizată.
- **Tranziție**: La finalizarea obiectivelor (câștig de 100 lei, cumpărare de țigări și energizant) starea devine `MISSION_02_COMPLETED`.
- **Următoarea Misiune Deblocată**: `Misiunea 3: La Gratar`.

---

## Note de Gameplay și Alegeri
- Misiunea pornește cu **25 lei** la păcănele.
- Recompensa finală este condiționată: poți termina cu **100 lei** dacă câștigi, sau cu **0 lei** dacă pierzi tot.
- Dacă obții 100 lei, Mario poate cere 50 de lei după joc.
- Dacă pierzi tot, Mario te poate taxa și te lasă fără bani.
- Plătirea lui Mario înseamnă respect -1; dacă refuzi, nu se întâmplă nimic imediat.
- Dacă îi dai boschetarului o țigară, primești **Respect +1 ascuns** și pierzi 1 leu.
- Misiunea poate fi finalizată chiar și fără profit, dar recompensa variază în funcție de rezultatul la păcănele și de alegerea cu boschetarul.
