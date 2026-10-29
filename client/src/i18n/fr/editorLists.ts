import type { Translation } from "../catalogue";

/** French strings for the editor's ingredient, step and field lists. */
export const editorLists = {
  "badge.check": "à vérifier",
  "row.more": "Plus",
  "row.duplicate": "Dupliquer",
  "section.add": "+ Ajouter une section",
  "section.label": "Section",
  "section.none": "Aucune section",
  "section.numbered": "Section {{number}}",

  "ingredients.heading": "Ingrédients",
  "ingredients.hint":
    "Saisissez une quantité et un nom ; appuyez sur Entrée pour passer à la ligne suivante.",
  "ingredients.add": "+ Ajouter un ingrédient",
  "ingredients.paste.title": "Coller une liste",
  "ingredients.paste.help":
    "Un ingrédient par ligne, quantité en premier (« 250 g de farine »). Une ligne terminée par « : » commence une section.",
  "ingredients.paste.placeholder":
    "250 g de farine\n1 c. à café de sel\nPour le glaçage :\n120 g de sucre glace",

  "ingredient.rowName": "ingrédient {{number}}",
  "ingredient.amount": "Quantité de l’ingrédient {{number}}",
  "ingredient.amountPlaceholder": "250 g",
  "ingredient.name": "Ingrédient {{number}}",
  "ingredient.namePlaceholder": "farine",
  "ingredient.preparation": "Préparation de l’ingrédient {{number}}",
  "ingredient.preparationPlaceholder": "tamisée",
  "ingredient.badge.optional": "facultatif",
  "ingredient.badge.fixed": "quantité fixe",
  "ingredient.badge.measures_one": "{{count}} mesure",
  "ingredient.badge.measures_many": "{{count}} mesures",
  "ingredient.badge.measures_other": "{{count}} mesures",
  "ingredient.badge.substitutes_one": "{{count}} substitut",
  "ingredient.badge.substitutes_many": "{{count}} substituts",
  "ingredient.badge.substitutes_other": "{{count}} substituts",
  "ingredient.note": "Remarque",
  "ingredient.notePlaceholder": "ou en conserve",
  "ingredient.optional": "Facultatif",
  "ingredient.fixed": "Quantité fixe",
  "ingredient.insertBelow": "Insérer en dessous",

  "measure.legend": "Autres mesures",
  "measure.label": "Autre mesure {{number}}",
  "measure.placeholder": "1 tasse",
  "measure.remove": "Supprimer la mesure {{number}}",
  "measure.add": "Ajouter une mesure",

  "substitute.legend": "Substituts",
  "substitute.amount": "Quantité du substitut {{number}}",
  "substitute.amountPlaceholder": "250 ml",
  "substitute.name": "Substitut {{number}}",
  "substitute.namePlaceholder": "babeurre",
  "substitute.remove": "Supprimer le substitut {{number}}",
  "substitute.add": "Ajouter un substitut",

  "steps.heading": "Étapes",
  "steps.add": "+ Ajouter une étape",
  "steps.paste.title": "Coller des étapes",
  "steps.paste.help":
    "Collez la préparation : une ligne vide sépare les étapes (ou une étape par ligne).",
  "steps.paste.placeholder":
    "Préchauffez le four à 180 °C.\n\nBattez le beurre et le sucre en crème.",

  "step.rowName": "étape {{number}}",
  "step.text": "Étape {{number}}",
  "step.textPlaceholder":
    "Ce qu’il faut faire. Une ligne vide commence un nouveau paragraphe.",
  "step.panel.ingredients": "Ingrédients utilisés",
  "step.panel.equipment": "Matériel utilisé",
  "step.panel.timers": "Minuteurs",
  "step.panel.heat": "Feu et température",
  "step.produces": "Donne (nomme le résultat)",
  "step.producesPlaceholder": "la pâte",
  "step.note": "Astuces",
  "step.notePlaceholder": "Ne laissez pas l’ail brunir.",
  "step.photo": "Photo de l’étape",
  "step.insertBelow": "Insérer une étape en dessous",

  "picker.ingredients.empty":
    "Ajoutez des ingrédients ci-dessus pour les choisir ici.",
  "picker.ingredients.legend": "Ingrédients utilisés dans cette étape",
  "picker.ingredients.portion": "Part de « {{name}} » utilisée ici",
  "picker.ingredients.portionFallback": "ingrédient",
  "picker.ingredients.portionPlaceholder":
    "tout (ou p. ex. la moitié : 1/2 tasse)",
  "picker.equipment.empty":
    "Ajoutez du matériel ci-dessus pour le choisir ici.",

  "timer.duration": "Durée du minuteur {{number}}",
  "timer.durationPlaceholder": "20 min",
  "timer.label": "Libellé du minuteur {{number}}",
  "timer.labelPlaceholder": "mijoter",
  "timer.passive": "Sans surveillance",
  "timer.remove": "Supprimer le minuteur {{number}}",
  "timer.add": "+ Ajouter un minuteur",

  "heat.label": "Feu",
  "heat.level.low": "doux",
  "heat.level.mediumLow": "moyen-doux",
  "heat.level.medium": "moyen",
  "heat.level.mediumHigh": "moyen-vif",
  "heat.level.high": "vif",
  "temp.min": "Température",
  "temp.max": "Jusqu’à (facultatif)",
  "temp.unit": "Unité de température",
  "temp.target": "Mesurée dans",
  "temp.target.oven": "le four",
  "temp.target.internal": "le cœur",
  "temp.target.oil": "l’huile",
  "temp.target.pan": "la poêle",
  "temp.target.water": "l’eau",

  "amount.unknownUnit":
    "Unité non reconnue — elle sera conservée telle quelle, sans conversion.",
  "amount.asText": "Conservé tel quel (aucun nombre à ajuster).",
  "amount.notDuration":
    "Ce n’est pas encore une durée — essayez 45 min ou 1h 30m.",
  "amount.notAmount": "Ce n’est pas encore une quantité.",
  "slug.remove": "Retirer {{slug}}",
} satisfies Translation<"editorLists">;
