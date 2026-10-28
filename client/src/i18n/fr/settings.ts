import type { Translation } from "../catalogue";

/** French strings for the settings screens. */
export const settings = {
  title: "Réglages",
  "language.title": "Langue",
  "language.label": "Langue de l’application",
  "language.system": "Comme cet appareil",
  "language.hint":
    "Change les boutons et les messages de l’application. Les recettes gardent leurs propres langues.",
  "identity.title": "Identité",
  "identity.locked": "Verrouillée",
  "identity.unlocked": "Déverrouillée",
  "identity.intro":
    "Votre identité signe les recettes que vous écrivez, pour que chacun sache qu’elles sont vraiment de vous.",
  "identity.backupWarning":
    "Votre clé n’existe que sur cet appareil. Sauvegardez-la — si vous la perdez, vous ne pourrez plus modifier vos recettes en tant que même auteur.",
  "identity.createNew": "Créer",
  "identity.restoreBackup": "Restaurer une sauvegarde",
  "identity.userId": "Identifiant",
  "identity.copyUserId": "Copier l’identifiant",
  "identity.unlockPrompt":
    "Déverrouillez votre clé pour signer des recettes et vous connecter aux serveurs.",
  "identity.unlockedNote":
    "Votre clé reste déverrouillée pour signer jusqu’à ce que vous la verrouilliez ou fermiez l’application.",
  "identity.lock": "Verrouiller",
  "identity.downloadBackup": "Télécharger une sauvegarde",
  "identity.forget": "Oublier sur cet appareil…",
  "identity.backupSaved":
    "Sauvegarde enregistrée. Conservez-la en lieu sûr, hors de cet appareil.",
  "identity.forgetTitle": "Oublier votre identité ?",
  "identity.forgetConfirm": "Oublier l’identité",
  "identity.forgetWord": "oublier",
  "identity.forgetBody":
    "Cela supprime votre clé de cet appareil. Sans sauvegarde, vous ne pourrez plus jamais modifier vos recettes en tant que même auteur.",
  "identity.noneYet": "Il n’y a pas encore d’identité sur cet appareil.",
  "displayName.label": "Nom affiché",
  "displayName.edit": "Modifier",
  "displayName.save": "Enregistrer",
  "displayName.cancel": "Annuler",
  "copy.copy": "Copier",
  "copy.copied": "Copié",
  "create.nameRequired":
    "Saisissez le nom sous lequel vos recettes seront signées.",
  "create.namePlaceholder": "p. ex. Mamie Rose",
  "create.confirmPassphrase": "Confirmer la phrase secrète",
  "create.creatingLabel": "Création de votre clé",
  "create.creating": "Création de votre clé…",
  "create.submit": "Créer l’identité",
  "import.missing":
    "Choisissez un fichier de sauvegarde et saisissez votre nom affiché.",
  "import.invalid": "Ce n’est pas un fichier de clé Crisp valide : {{error}}",
  "import.file": "Fichier de sauvegarde (.crispkey)",
  "import.restoring": "Restauration",
  "import.submit": "Restaurer l’identité",
  "import.hint":
    "Vous la déverrouillerez avec la phrase secrète choisie lors de sa création.",
  "passphrase.label": "Phrase secrète",
  "passphrase.show": "Afficher la phrase secrète",
  "passphrase.showButton": "Afficher",
  "passphrase.hideButton": "Masquer",
  "unlock.failed":
    "Cette phrase secrète n’a pas déverrouillé votre clé. Réessayez.",
  "unlock.unlockingLabel": "Déverrouillage",
  "unlock.unlocking": "Déverrouillage…",
  "unlock.submit": "Déverrouiller",
  "server.title": "Serveur",
  "server.status.idle": "Non vérifié",
  "server.status.connecting": "Connexion…",
  "server.status.online": "En ligne",
  "server.status.offline": "Hors ligne",
  "server.intro":
    "Facultatif. Un serveur vous permet de publier des recettes, de parcourir celles des autres et de les faire approuver. Tout le reste fonctionne hors ligne.",
  "server.address": "Adresse du serveur",
  "server.badAddress": "Saisissez une adresse http:// ou https://.",
  "server.retry": "Réessayer",
  "server.connect": "Se connecter",
  "server.offline":
    "Impossible de joindre ce serveur pour le moment. Votre bibliothèque continue de fonctionner hors ligne.",
  "server.offlineWithError":
    "Impossible de joindre ce serveur pour le moment ({{error}}). Votre bibliothèque continue de fonctionner hors ligne.",
  "server.keyMismatch":
    "<strong>Les clés de signature de ce serveur ont changé.</strong> Cela peut arriver après une rotation des clés — ou ce n’est peut-être pas le serveur auquel vous faisiez confiance. Ne faites confiance aux nouvelles clés que si vous vous y attendiez.",
  "server.trustKeys": "Faire confiance aux nouvelles clés",
  "server.core": "Cœur du serveur",
  "server.formatVersion": "Version du format",
  "server.signedInAs": "Connecté en tant que <strong>{{username}}</strong>",
  "server.signOut": "Se déconnecter",
  "server.needIdentity":
    "Créez une identité ci-dessus pour vous connecter et publier des recettes.",
  "server.unlockPrompt":
    "Déverrouillez votre identité pour vous connecter à ce serveur.",
  "server.unlockFirst": "Déverrouillez d’abord votre identité.",
  "server.username": "Nom d’utilisateur",
  "server.signingIn": "Connexion",
  "server.signIn": "Se connecter",
  "storage.title": "Stockage et application",
  "storage.keepLibrary": "Conserver ma bibliothèque",
  "storage.persisted":
    "Stockage persistant — le navigateur ne l’effacera pas pour libérer de l’espace.",
  "storage.notPersisted":
    "Demander au navigateur de ne pas effacer vos recettes quand l’espace manque.",
  "storage.persistent": "Persistant",
  "storage.keep": "Conserver",
  "storage.spaceUsed": "Espace utilisé",
  "storage.usage": "{{used}} sur {{quota}} disponibles",
  "storage.notReported": "Non communiqué par ce navigateur.",
  "storage.app": "Application",
  "storage.core": "Cœur Crisp",
  "storage.unavailable": "indisponible",
  "storage.format": "Format",
  "storage.spec": "Lire la spécification du format Crisp",
  "version.reads": "{{version}} (lit {{min}}+)",
} satisfies Translation<"settings">;
