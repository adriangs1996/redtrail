#!/bin/sh
# RedTrail dev container MOTD — Catppuccin Mocha palette

# Catppuccin Mocha ANSI 256-color approximations
RED='\033[38;2;243;139;168m'      # #f38ba8
MAROON='\033[38;2;235;160;172m'   # #eba0ac
PEACH='\033[38;2;250;179;135m'    # #fab387
YELLOW='\033[38;2;249;226;175m'   # #f9e2af
GREEN='\033[38;2;166;227;161m'    # #a6e3a1
TEAL='\033[38;2;148;226;213m'     # #94e2d5
SKY='\033[38;2;137;220;235m'      # #89dceb
SAPPHIRE='\033[38;2;116;199;236m' # #74c7ec
BLUE='\033[38;2;137;180;250m'     # #89b4fa
LAVENDER='\033[38;2;180;190;254m' # #b4befe
MAUVE='\033[38;2;203;166;247m'    # #cba6f7
PINK='\033[38;2;245;194;231m'     # #f5c2e7
TEXT='\033[38;2;205;214;244m'     # #cdd6f4
SUBTEXT='\033[38;2;166;173;200m'  # #a6adc8
SURFACE='\033[38;2;69;71;90m'     # #45475a
DIM='\033[2m'
BOLD='\033[1m'
RESET='\033[0m'

# Get redtrail version
RT_VERSION=$(redtrail --version 2>/dev/null || echo "unknown")

printf "\n"
printf "${SURFACE}  ──────────────────────────────────────────────────────────  ${RESET}\n"
printf "\n"
printf "${RED}        ██████${PEACH}  ███████${YELLOW} ██████${GREEN}  ████████${TEAL} ██████${SAPPHIRE}   █████  ${BLUE} ██${LAVENDER} ██     ${RESET}\n"
printf "${RED}        ██   ██${PEACH} ██     ${YELLOW} ██   ██${GREEN}    ██   ${TEAL} ██   ██${SAPPHIRE}  ██   ██ ${BLUE} ██${LAVENDER} ██     ${RESET}\n"
printf "${RED}        ██████${PEACH}  █████  ${YELLOW} ██   ██${GREEN}    ██   ${TEAL} ██████${SAPPHIRE}  ███████ ${BLUE} ██${LAVENDER} ██     ${RESET}\n"
printf "${RED}        ██   ██${PEACH} ██     ${YELLOW} ██   ██${GREEN}    ██   ${TEAL} ██   ██${SAPPHIRE}  ██   ██ ${BLUE} ██${LAVENDER} ██     ${RESET}\n"
printf "${RED}        ██   ██${PEACH} ███████${YELLOW} ██████${GREEN}     ██   ${TEAL} ██   ██${SAPPHIRE}  ██   ██ ${BLUE} ██${LAVENDER} ██████ ${RESET}\n"
printf "\n"
printf "${SURFACE}  ──────────────────────────────────────────────────────────  ${RESET}\n"
printf "\n"
printf "${MAUVE}  ${BOLD}dev container${RESET}${SUBTEXT}  ·  ${TEXT}${RT_VERSION}${RESET}\n"
printf "\n"
printf "${SKY}    shell  ${SUBTEXT}zsh + redtrail hook loaded${RESET}\n"
printf "${GREEN}    tools  ${SUBTEXT}git, curl, node $(node -v 2>/dev/null), claude-code${RESET}\n"
printf "${PEACH}    tests  ${SUBTEXT}/tests/*.sh${RESET}\n"
printf "\n"
printf "${SURFACE}  ──────────────────────────────────────────────────────────  ${RESET}\n"
printf "\n"
