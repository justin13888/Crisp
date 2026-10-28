import type { Translation } from "../catalogue";

/** French strings for the browse screens. */
export const browse = {
  title: "Parcourir",
  "offline.connecting": "Connexion à {{url}}…",
  "offline.title": "Aucune connexion au serveur",
  "offline.body":
    "Cet écran présente les recettes que d’autres ont publiées sur un serveur Crisp. Impossible de joindre <strong>{{url}}</strong> pour le moment — vous êtes peut-être hors ligne, ou le serveur est en panne.",
  "offline.local":
    "Votre propre bibliothèque fonctionne entièrement hors ligne.",
  "offline.retry": "Réessayer",
  "offline.settings": "Réglages du serveur",
  "search.label": "Rechercher des recettes publiées",
  "search.submit": "Rechercher",
  "filters.title": "Filtres",
  "filters.titleCount": "Filtres ({{number}})",
  "filters.course": "Plat",
  "filters.diet": "Régime",
  "filters.allergenFree": "Sans",
  "filters.clear": "Tout effacer",
  loadFailed: "Impossible de charger les recettes : {{error}}",
  "empty.filtered": "Aucune recette publiée ne correspond à ces filtres.",
  "empty.none": "Rien n’a encore été publié ici.",
  loading: "Chargement…",
  loadMore: "Charger plus",
  "result.by": "par {{authors}}",
  "result.tags": "Étiquettes",
  "result.inLibrary": "Dans votre bibliothèque",
  "result.open": "Ouvrir",
  "result.saving": "Enregistrement",
  "result.save": "Ajouter à la bibliothèque",
  "result.gone": "Le serveur n’a plus cette recette.",
} satisfies Translation<"browse">;
