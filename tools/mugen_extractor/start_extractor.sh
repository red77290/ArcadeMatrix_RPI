#!/bin/bash
echo "====================================================="
echo "   ArcadeMatrix RPi MUGEN / Sprite Character Extractor"
echo "====================================================="

# Go to script directory
cd "$(dirname "$0")" || exit

if [ ! -d "venv" ]; then
    echo "[INFO] Creating Python Virtual Environment..."
    python3 -m venv venv
    if [ $? -ne 0 ]; then
        echo "[ERROR] python3 is not installed or not in PATH!"
        exit 1
    fi
fi

echo "[INFO] Activating virtual environment..."
source venv/bin/activate

echo "[INFO] Installing requirements..."
pip install -r requirements.txt -q

echo ""
# 1. Input Folder
default_input="./chars"
read -e -p "📁 Dossier Source MUGEN (contenant les personnages) [$default_input]: " input_folder
input_folder="${input_folder:-$default_input}"

# 2. Output Folder
default_output="./fighters_64"
read -e -p "📁 Dossier Destination (ex: ./fighters_64) [$default_output]: " output_folder
output_folder="${output_folder:-$default_output}"

# 3. Scaling / Échelle
echo ""
echo "-----------------------------------------------------"
echo "📏 Choix du Facteur d'Échelle (Scaling Factor) :"
echo "   • 1.0   : Recommandé Raspberry Pi / Matrice 64px+ (Taille 1:1 d'origine)"
echo "   • 0.5   : Échelle 50% (pour matrices 32px de haut)"
echo "   • auto  : Ajustement automatique proportionnel à la hauteur de l'écran"
echo "   • Ou entrez une valeur personnalisée (ex: 0.4, 0.75, 1.25, 2.0)"
echo "-----------------------------------------------------"
read -p "Échelle souhaitée [défaut: 1.0]: " scale_input
scale_input="${scale_input:-1.0}"

# 4. Compression
echo ""
echo "-----------------------------------------------------"
echo "🗜️  Compression des fichiers (.fgt vs .fgt.gz) :"
echo "   • y (Oui) : Recommandé pour Raspberry Pi (gain ~80% d'espace disque)"
echo "   • n (Non) : Fichiers bruts non compressés"
echo "-----------------------------------------------------"
read -p "Compresser en .fgt.gz ? (y/n) [défaut: y]: " compress_input
compress_input="${compress_input:-y}"

# Build arguments
EXTRA_ARGS=""
if [ "$scale_input" = "auto" ] || [ "$scale_input" = "scaled" ] || [ "$scale_input" = "SCALED" ]; then
    EXTRA_ARGS="--mode SCALED"
elif [ "$scale_input" = "1.0" ] || [ "$scale_input" = "full" ] || [ "$scale_input" = "FULLSIZE" ]; then
    EXTRA_ARGS="--scale 1.0"
else
    EXTRA_ARGS="--scale $scale_input"
fi

if [ "$compress_input" = "y" ] || [ "$compress_input" = "Y" ] || [ "$compress_input" = "yes" ]; then
    EXTRA_ARGS="$EXTRA_ARGS --compress"
fi

echo ""
echo "🚀 Lancement de l'extraction :"
echo "   • Source      : $input_folder"
echo "   • Destination : $output_folder"
echo "   • Paramètres  : $EXTRA_ARGS"
echo ""

python mugen_extractor.py -i "$input_folder" -o "$output_folder" $EXTRA_ARGS

echo ""
echo "✅ [TERMINÉ] Extraction terminée."
