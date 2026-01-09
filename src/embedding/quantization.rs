//! Vector quantization module for reducing memory usage
//!
//! Implements f16 (half-precision) quantization which reduces memory by 50%
//! with minimal loss in search quality (<0.1% degradation in cosine similarity).

use anyhow::Result;

/// Quantize f32 vector to f16 (IEEE 754 half-precision)
/// Reduces memory by 50% (4 bytes → 2 bytes per value)
pub fn quantize_f32_to_f16(vector: &[f32]) -> Vec<u8> {
    let mut quantized = Vec::with_capacity(vector.len() * 2);
    
    for &value in vector {
        // Convert f32 to f16 using bit manipulation
        let bits = value.to_bits();
        let f16_bits = f32_to_f16_bits(bits);
        
        // Store as 2 bytes (little-endian)
        quantized.push((f16_bits & 0xFF) as u8);
        quantized.push((f16_bits >> 8) as u8);
    }
    
    quantized
}

/// Dequantize f16 back to f32
pub fn dequantize_f16_to_f32(quantized: &[u8]) -> Result<Vec<f32>> {
    if !quantized.len().is_multiple_of(2) {
        anyhow::bail!("Invalid quantized vector length: must be even");
    }
    
    let mut vector = Vec::with_capacity(quantized.len() / 2);
    
    for chunk in quantized.chunks_exact(2) {
        // Read 2 bytes as little-endian u16
        let f16_bits = u16::from_le_bytes([chunk[0], chunk[1]]);
        
        // Convert f16 to f32
        let f32_bits = f16_to_f32_bits(f16_bits);
        let value = f32::from_bits(f32_bits);
        
        vector.push(value);
    }
    
    Ok(vector)
}

/// Convert f32 bits to f16 bits (IEEE 754 half-precision)
/// Reference: https://en.wikipedia.org/wiki/Half-precision_floating-point_format
fn f32_to_f16_bits(f32_bits: u32) -> u16 {
    // Extract components
    let sign = (f32_bits >> 31) & 0x1;
    let exponent = ((f32_bits >> 23) & 0xFF) as i32;
    let mantissa = f32_bits & 0x7FFFFF;
    
    // Handle special cases
    if exponent == 0xFF {
        // Infinity or NaN
        let nan_bit = if mantissa != 0 { 1u32 } else { 0u32 };
        return ((sign << 15) | 0x7C00 | nan_bit) as u16;
    }
    
    if exponent == 0 && mantissa == 0 {
        // Zero
        return (sign << 15) as u16;
    }
    
    // Convert exponent (f32: bias 127, f16: bias 15)
    let f16_exp = exponent - 127 + 15;
    
    // Handle overflow/underflow
    if f16_exp >= 0x1F {
        // Overflow to infinity
        return ((sign << 15) | 0x7C00) as u16;
    }
    
    if f16_exp <= 0 {
        // Underflow to zero or denormal
        if f16_exp < -10 {
            // Too small, flush to zero
            return (sign << 15) as u16;
        }
        // Denormal number
        let shift = 1 - f16_exp;
        let f16_mantissa = (mantissa | 0x800000) >> (shift + 13);
        return ((sign << 15) | f16_mantissa) as u16;
    }
    
    // Normal number
    let f16_mantissa = mantissa >> 13;
    ((sign << 15) | ((f16_exp as u32) << 10) | f16_mantissa) as u16
}

/// Convert f16 bits to f32 bits
fn f16_to_f32_bits(f16_bits: u16) -> u32 {
    let sign = ((f16_bits >> 15) & 0x1) as u32;
    let exponent = ((f16_bits >> 10) & 0x1F) as i32;
    let mantissa = (f16_bits & 0x3FF) as u32;
    
    // Handle special cases
    if exponent == 0x1F {
        // Infinity or NaN
        return (sign << 31) | 0x7F800000 | (mantissa << 13);
    }
    
    if exponent == 0 {
        if mantissa == 0 {
            // Zero
            return sign << 31;
        }
        // Denormal f16 -> normal f32
        let mut exp = -14;
        let mut mant = mantissa;
        while (mant & 0x400) == 0 {
            mant <<= 1;
            exp -= 1;
        }
        mant &= 0x3FF;
        let f32_exp = (exp + 127) as u32;
        return (sign << 31) | (f32_exp << 23) | (mant << 13);
    }
    
    // Normal number
    let f32_exp = (exponent - 15 + 127) as u32;
    (sign << 31) | (f32_exp << 23) | (mantissa << 13)
}

/// Calculate quantization error (L2 distance)
pub fn calculate_quantization_error(original: &[f32], quantized: &[f32]) -> f32 {
    original
        .iter()
        .zip(quantized.iter())
        .map(|(a, b)| {
            let diff = a - b;
            diff * diff
        })
        .sum::<f32>()
        .sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantize_dequantize_roundtrip() {
        let original = vec![0.1, 0.5, -0.3, 1.0, -1.0, 0.0];
        let quantized = quantize_f32_to_f16(&original);
        let dequantized = dequantize_f16_to_f32(&quantized).unwrap();

        assert_eq!(original.len(), dequantized.len());

        // Check that error is small
        for (o, d) in original.iter().zip(dequantized.iter()) {
            let error = (o - d).abs();
            assert!(error < 0.001, "Error too large: {} vs {}", o, d);
        }
    }

    #[test]
    fn test_quantize_embedding() {
        // Simulate a 384-dimensional embedding
        let embedding: Vec<f32> = (0..384).map(|i| (i as f32) / 384.0 - 0.5).collect();
        let quantized = quantize_f32_to_f16(&embedding);
        
        // Check size reduction
        assert_eq!(quantized.len(), embedding.len() * 2); // 50% reduction
        
        let dequantized = dequantize_f16_to_f32(&quantized).unwrap();
        let error = calculate_quantization_error(&embedding, &dequantized);
        
        // Error should be very small
        assert!(error < 0.01, "Quantization error too large: {}", error);
    }

    #[test]
    fn test_special_values() {
        let values = vec![0.0, -0.0, 1.0, -1.0, f32::INFINITY, f32::NEG_INFINITY];
        let quantized = quantize_f32_to_f16(&values);
        let dequantized = dequantize_f16_to_f32(&quantized).unwrap();

        for (o, d) in values.iter().zip(dequantized.iter()) {
            if o.is_nan() {
                assert!(d.is_nan());
            } else {
                assert_eq!(o, d, "Special value mismatch: {} vs {}", o, d);
            }
        }
    }

    #[test]
    fn test_cosine_similarity_preservation() {
        use crate::embedding::EmbeddingEngine;

        let v1: Vec<f32> = (0..384).map(|i| (i as f32).sin()).collect();
        let v2: Vec<f32> = (0..384).map(|i| (i as f32).cos()).collect();

        // Original similarity
        let original_sim = EmbeddingEngine::cosine_similarity(&v1, &v2);

        // Quantized similarity
        let q1 = quantize_f32_to_f16(&v1);
        let q2 = quantize_f32_to_f16(&v2);
        let dq1 = dequantize_f16_to_f32(&q1).unwrap();
        let dq2 = dequantize_f16_to_f32(&q2).unwrap();
        let quantized_sim = EmbeddingEngine::cosine_similarity(&dq1, &dq2);

        // Similarity should be preserved within 0.1%
        let diff = (original_sim - quantized_sim).abs();
        assert!(
            diff < 0.001,
            "Cosine similarity degraded too much: {} -> {} (diff: {})",
            original_sim,
            quantized_sim,
            diff
        );
    }
}
