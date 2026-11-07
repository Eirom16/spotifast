//! Prototipo histórico de callback HTTP local; no es un navegador embebido.
//! No se conecta al flujo de producto: un origen localhost no recibe cookies
//! de youtube.com. No demuestra una cuenta musical autenticada.
//!
//! Este módulo implementa un servidor HTTP local que:
//! 1. Abre el navegador del sistema con la URL de login de YouTube Music
//! 2. YouTube redirige de vuelta a nuestro servidor local (callback)
//! 3. Extrae las cookies de la petición
//! 4. Cierra el navegador automáticamente
//!
//! RESTRICCIÓN ESTRICTA: Este módulo solo puede usarse para el flujo de
//! autenticación OAuth. El servidor se destruye inmediatamente después de
//! obtener las credenciales.
//!
//! Ver: docs/_decisions/2025-embedded-browser-auth.md

use crate::authorization;
use doremi_core::catalog::ProviderError as Error;
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const LOGIN_URL: &str = "https://accounts.google.com/ServiceLogin?service=youtube&continue=https%3A%2F%2Fmusic.youtube.com%2F";
const CALLBACK_PATH: &str = "/auth/callback";

/// Inicia el flujo de autenticación usando servidor HTTP local
/// Este método abre el navegador del sistema y espera el callback en un servidor local
pub fn consent_embedded(cancel: &AtomicBool, _base: &std::path::Path) -> Result<String, Error> {
    if cancel.load(Ordering::Acquire) {
        return Err(Error::Cancelled);
    }
    // Crear un listener en un puerto aleatorio
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .map_err(|_| Error::Unavailable)?;

    let local_addr = listener.local_addr().map_err(|_| Error::Unavailable)?;
    let callback_url = format!("http://{}{}", local_addr, CALLBACK_PATH);

    // URL de login con callback
    let full_login_url = format!("{LOGIN_URL}&continue={}", callback_url);

    // Abrir el navegador del sistema
    open_system_browser(&full_login_url)?;

    // Esperar conexión
    listener
        .set_nonblocking(true)
        .map_err(|_| Error::Unavailable)?;
    let deadline = Instant::now() + Duration::from_secs(600);
    let mut stream = loop {
        if cancel.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(Error::Timeout);
        }
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50))
            }
            Err(_) => return Err(Error::Unavailable),
        }
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|_| Error::Unavailable)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|_| Error::Unavailable)?;

    // Leer la petición HTTP
    let mut buffer = [0u8; 8192];
    let bytes_read = stream.read(&mut buffer).map_err(|_| Error::Unavailable)?;

    let request = String::from_utf8_lossy(&buffer[..bytes_read]);

    // Extraer cookies de la petición
    let cookie = extract_cookies_from_request(&request);

    // Verificar que la cookie es válida
    if let Some(cookie) = cookie
        && authorization(&cookie, 0).is_some()
    {
        // Enviar respuesta para cerrar el navegador
        let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n<html><body><p>Credenciales recibidas. La cuenta musical todavia debe verificarse en Doremi.</p></body></html>";
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.flush();

        drop(stream);
        drop(listener);
        return Ok(cookie);
    }

    // Si no obtuvimos cookies válidas, cerrar
    let response = "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n<html><body><h1>Error de autenticación</h1><p>No se pudo iniciar sesión. Por favor, inténtalo de nuevo.</p><script>window.close();</script></body></html>";
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();

    drop(stream);
    drop(listener);
    Err(Error::InvalidResponse)
}

/// Extrae cookies de una petición HTTP
fn extract_cookies_from_request(request: &str) -> Option<String> {
    for line in request.lines() {
        if line.to_lowercase().starts_with("cookie:") {
            return Some(line[7..].trim().to_string());
        }
    }
    None
}

/// Abre el navegador del sistema
fn open_system_browser(url: &str) -> Result<(), Error> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map_err(|_| Error::Unavailable)?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|_| Error::Unavailable)?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|_| Error::Unavailable)?;
    }

    Ok(())
}
