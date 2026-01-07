/// Simple greedy clustering based on cosine similarity of embeddings.
/// This is lightweight (no k-means) and suitable for limited-memory environments.

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 { return 0.0; }
    dot / (na * nb)
}

/// Cluster fragments by threshold: iterate through fragments and assign to existing
/// cluster if similarity to cluster centroid >= threshold, otherwise create new cluster.
pub fn cluster_by_threshold(embeddings: &[(String, Vec<f32>)], threshold: f32) -> Vec<Vec<String>> {
    let mut clusters: Vec<(Vec<f32>, Vec<String>)> = Vec::new();

    for (id, emb) in embeddings {
        let mut placed = false;
        for (centroid, ids) in clusters.iter_mut() {
            let sim = cosine_similarity(centroid, emb);
            if sim >= threshold {
                // update centroid (simple mean)
                for (i, v) in emb.iter().enumerate() {
                    centroid[i] = (centroid[i] + v) / 2.0;
                }
                ids.push(id.clone());
                placed = true;
                break;
            }
        }

        if !placed {
            clusters.push((emb.clone(), vec![id.clone()]));
        }
    }

    clusters.into_iter().map(|(_, ids)| ids).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_sim() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &b) > 0.9);
        assert!(cosine_similarity(&a, &c) < 0.1);
    }
}
