# Terragrunt TUI

Interface terminal qui agrège le résultat d'un `terragrunt run --all plan` et le présente
par module, par ressource et par modification, pour relire un plan avant de l'appliquer.

## Language

### Plan

**Module** :
Une unité Terragrunt (un dossier contenant un `terragrunt.hcl`) planifiée lors de l'exécution.
_Avoid_ : unité, unit, stack

**Vague** :
Niveau d'éligibilité d'un module dans le graphe de dépendances : tous les modules d'une même
vague ne dépendent que de modules de vagues précédentes. Ce n'est pas un ordre d'exécution
réel. Un module hors graphe appartient à la vague inconnue.
_Avoid_ : wave, groupe, étape, batch

**Ressource** :
Un objet d'infrastructure adressé dans le plan d'un module (`aws_vpc.main`).
_Avoid_ : resource, objet, item

**Action** :
Ce que le plan prévoit pour une ressource : création, modification, suppression,
remplacement, lecture, aucun changement, ou inconnu.
_Avoid_ : opération, change kind

**Modification** :
La différence entre l'état avant et l'état après d'une ressource, telle que le plan la
prévoit. Elle se présente sous forme de diff.
_Avoid_ : changement, delta, changes

### Écran de revue

**Panneau** :
Une des trois colonnes de l'écran de revue : Modules, Ressources, Modifications. Un seul
panneau a le focus à la fois.
_Avoid_ : colonne, pane, zone, vue

**Panneau réduit** :
État d'un panneau situé à gauche du panneau qui a le focus : il garde sa structure, sa
sélection et ses compteurs, mais abrège ses libellés pour laisser la place.
_Avoid_ : compacté, replié, minimisé

**Statut** :
Résultat de la planification d'un module : planned, no changes, failed ou unknown.
_Avoid_ : état, résultat, outcome
