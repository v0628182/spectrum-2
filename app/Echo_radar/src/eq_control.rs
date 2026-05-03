/// eq_control.rs — Control en runtime de Equalizer APO desde EchoAudio
///
/// Este módulo permite a EchoAudio.exe cambiar el perfil de EQ sin
/// ninguna ventana ni interacción del usuario. Simplemente reescribe
/// el config.txt del APO y el motor de audio de Windows lo detecta
/// automáticamente via ReadDirectoryChangesW.

use std::fs;
use std::path::PathBuf;
use anyhow::{Context, Result};

/// Perfiles disponibles de EchoAudio
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EchoProfile {
    /// Perfil 1: Gaming/Footsteps — Preamp +7.7dB, MJUCjr compress 0.5,
    ///           HP 118Hz + boost 125-250Hz y 6300-8000Hz (pasos de pies)
    Footsteps,
    /// Perfil 2: Compresión agresiva — MJUCjr 0.965, HP 70Hz, sub-bass boost
    Compressed,
    /// Perfil 3: Loudness EQ — MJUCjr moderado + corrección de curva GraphicEQ
    LoudnessEQ,
    /// Perfil 4: Full — GraphicEQ completo + LoudnessCorrection (sin compresor)
    Full,
    /// Sin procesamiento (bypass total)
    Bypass,
}

impl EchoProfile {
    /// Nombre del archivo de perfil en el directorio config del APO
    pub fn filename(&self) -> &'static str {
        match self {
            Self::Footsteps   => "echo_profile1.txt",
            Self::Compressed  => "echo_profile2.txt",
            Self::LoudnessEQ  => "echo_profile3.txt",
            Self::Full        => "echo_profile4.txt",
            Self::Bypass      => "echo_bypass.txt",
        }
    }

    /// Nombre legible para logging interno (nunca mostrado al usuario)
    pub fn name(&self) -> &'static str {
        match self {
            Self::Footsteps   => "Footsteps",
            Self::Compressed  => "Compressed",
            Self::LoudnessEQ  => "LoudnessEQ",
            Self::Full        => "Full",
            Self::Bypass      => "Bypass",
        }
    }
}

/// Controller de Equalizer APO
pub struct EqController {
    config_path: PathBuf,
    current_profile: EchoProfile,
}

impl EqController {
    /// Intenta crear el controller apuntando al config.txt de APO.
    /// Devuelve None si el APO no está instalado (la app continúa sin EQ).
    pub fn try_new() -> Option<Self> {
        let config_path = PathBuf::from(
            r"C:\Program Files\EqualizerAPO\config\config.txt"
        );

        if config_path.exists() {
            Some(Self {
                config_path,
                current_profile: EchoProfile::Footsteps,
            })
        } else {
            // APO no instalado — EchoAudio funciona sin él
            tracing::warn!("EqualizerAPO no detectado — EQ deshabilitado");
            None
        }
    }

    /// Cambia el perfil activo reescribiendo config.txt.
    ///
    /// El APO detecta el cambio vía ReadDirectoryChangesW y aplica el
    /// nuevo perfil en <100ms sin interrumpir el audio.
    pub fn set_profile(&mut self, profile: EchoProfile) -> Result<()> {
        if profile == self.current_profile {
            return Ok(());
        }

        let content = self.build_config(profile);

        fs::write(&self.config_path, &content)
            .with_context(|| format!("No se pudo escribir config.txt en {:?}", self.config_path))?;

        tracing::debug!("EQ perfil aplicado: {}", profile.name());
        self.current_profile = profile;
        Ok(())
    }

    /// Devuelve el perfil actualmente activo
    #[allow(dead_code)]
    pub fn current_profile(&self) -> EchoProfile {
        self.current_profile
    }

    /// Construye el contenido de config.txt para un perfil dado
    fn build_config(&self, profile: EchoProfile) -> String {
        if profile == EchoProfile::Bypass {
            // Bypass: archivo vacío = sin procesamiento (APO pasa audio sin tocar)
            return "# EchoAudio — Bypass\n".to_string();
        }

        format!(
            "# EchoAudio Audio Engine — Managed Configuration\n\
             # Profile: {}\n\
             # DO NOT EDIT MANUALLY\n\
             Include: {}\n",
            profile.name(),
            profile.filename()
        )
    }

    /// Verifica que el APO está cargado correctamente leyendo el config actual
    #[allow(dead_code)]
    pub fn verify_loaded(&self) -> bool {
        fs::read_to_string(&self.config_path)
            .map(|content| {
                let expected_file = self.current_profile.filename();
                content.contains(expected_file) || self.current_profile == EchoProfile::Bypass
            })
            .unwrap_or(false)
    }
}

/// Función de conveniencia: aplica el perfil Footsteps al inicio de EchoAudio
/// (llamar desde main.rs antes de iniciar el overlay)
pub fn init_default_profile() -> Option<EqController> {
    let mut ctrl = EqController::try_new()?;
    
    if let Err(e) = ctrl.set_profile(EchoProfile::Footsteps) {
        tracing::error!("Error aplicando perfil EQ por defecto: {}", e);
    }
    
    Some(ctrl)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_content_footsteps() {
        let ctrl = EqController {
            config_path: PathBuf::from("dummy"),
            current_profile: EchoProfile::Bypass,
        };
        let content = ctrl.build_config(EchoProfile::Footsteps);
        assert!(content.contains("echo_profile1.txt"));
        assert!(content.contains("Footsteps"));
    }

    #[test]
    fn config_content_bypass() {
        let ctrl = EqController {
            config_path: PathBuf::from("dummy"),
            current_profile: EchoProfile::Bypass,
        };
        let content = ctrl.build_config(EchoProfile::Bypass);
        assert!(content.contains("Bypass"));
        assert!(!content.contains("Include:"));
    }

    #[test]
    fn all_profiles_have_filename() {
        let profiles = [
            EchoProfile::Footsteps,
            EchoProfile::Compressed,
            EchoProfile::LoudnessEQ,
            EchoProfile::Full,
            EchoProfile::Bypass,
        ];
        for p in profiles {
            assert!(!p.filename().is_empty());
            assert!(!p.name().is_empty());
        }
    }
}
