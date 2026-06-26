# Misiunea 13: Operațiunea Fierul Vechi
**Sezon / Episod corelat**: Sezonul 1 (GTA Vice City Style)
**Client / Giver**: Haralambie Olaru
**Recompense**: 2 capace de canal din fontă (complet inutile), 50 RON, Respect +5
**Prerechizite (Prerequisites)**: Misiunea 12: TBD

---

## Rezumat și Obiective
**Obiectiv Principal**: Haralambie îl contactează în secret pe Neimar, convins că a dat peste „Sindicatul Capacelor de Fontă” – o rețea de sabotaj economic condusă de Nea Gică, un căruțaș local care adună fier vechi. Neimar trebuie să-l ajute pe Haralambie să fileze căruța lui Nea Gică în Pantelimon și să recupereze două capace de canal furate, totul într-o urmărire plină de paranoia de mare viteză (la 40 km/h).

### Obiective de Gameplay:
- [ ] Întâlnește-te cu Haralambie în spatele ghenei de la blocul 4.
- [ ] Condu Loganul lui Haralambie și filează căruța lui Nea Gică de la distanță.
- [ ] Infiltrează-te în curtea centrului de colectare fier vechi și recuperează capacele.
- [ ] Livrează capacele în siguranță la Secția 23 de Poliție.

---

## Dialoguri în Română (Slang de Pantelimon)
```dialogue
- Haralambie: „Neimar, stinge farurile! Să nu ne vadă Nea Gică de pe cal! Aici e filaj profesionist, nu ne jucăm.”
- Neimar: „Hari, e ora două după-amiaza, ești într-un Logan galben cu numere de MAI și miroși a covrigi de la o poștă.”
- Haralambie (când ajung la secție): „Șefu', am destructurat sindicatul! Am recuperat fonta patriei!”
- Șeful de secție: „Olarule, ești cretin? Nea Gică e bunicul meu. Du capacele înapoi și pune-le la loc până nu te trimit la curățat WC-uri!”
```

---

## Storyboard (Cadre Manga/Hentai - Prompturi Vizuale)
Acest storyboard conține descrierea cadrelor vizuale care ilustrează acțiunea misiunii pentru a ghida artistul grafic:

- **[Cadru 1] Haralambie Olaru stă pitit în spatele unei ghene de gunoi din Pantelimon, purtând ochelari de soare negri în plină zi.**
- **[Cadru 2] Neimar conduce Loganul galben de miliție în timp ce Haralambie stă pe bancheta din spate, uitându-se paranoic prin binoclu la o căruță trasă de un cal leneș.**
- **[Cadru 3] Șeful de secție dă de pământ cu chipiul lui Haralambie, în timp ce Neimar zâmbește în colțul gurii sprijinit de perete.**

---

## Mașina de Stări a Jocului (Game State Machine)
- **Stare curentă**: `MISSION_13_ACTIVE`
- **Condiție de deblocare**: `Misiunea 12: Nemulțumirea Căpitanului` finalizată.
- **Tranziție**: La finalizarea tuturor obiectivelor și raportarea la secție, starea devine `MISSION_13_COMPLETED`.
- **Următoarea Misiune Deblocată**: `Misiunea 14: Încolțit`.
