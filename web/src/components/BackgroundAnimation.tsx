import React, { useEffect, useRef } from "react";
import * as THREE from 'three';

export const BackgroundAnimation: React.FC = () => {
    const containerRef = useRef<HTMLDivElement | null>(null);

    useEffect(() => {
        if (!containerRef.current) return;

        const scene = new THREE.Scene();
        const background = '#101213';
        scene.fog = new THREE.Fog(background, 1, 300000);

        const camera = new THREE.PerspectiveCamera(
            55,
            window.innerWidth / window.innerHeight,
            1,
            400000
        );
        camera.position.set(0, 10000, 10000);

        // renderer
        const renderer = new THREE.WebGLRenderer({ alpha: false });
        renderer.setSize(window.innerWidth, window.innerHeight);
        renderer.setClearColor(background, 1);
        containerRef.current.appendChild(renderer.domElement);

        // plane
        const vertexHeight = 15000;
        const planeDefinition = 100;
        const planeSize = 1245000;
        const meshColor = '#303030';

        const planeGeometry = new THREE.PlaneGeometry(
            planeSize,
            planeSize,
            planeDefinition,
            planeDefinition
        );
        const planeMaterial = new THREE.MeshBasicMaterial({
            color: meshColor,
            wireframe: true,
        });
        const plane = new THREE.Mesh(planeGeometry, planeMaterial);
        plane.rotation.x = -Math.PI / 2;
        scene.add(plane);

        // init
        const positionAttribute = planeGeometry.attributes.position;
        const vertexCount = positionAttribute.count;
        const initialZPositions = new Float32Array(vertexCount);

        // Initialize plane vertices
        for (let i = 0; i < vertexCount; i++) {
            let z = positionAttribute.getZ(i);
            z += Math.random() * vertexHeight - vertexHeight;
            positionAttribute.setZ(i, z);
            initialZPositions[i] = z;
        }
        positionAttribute.needsUpdate = true;

        // animate
        let count = 0;
        let animationFrameId: number;

        const animate = () => {
            animationFrameId = requestAnimationFrame(animate);

            // rotate camera
            const rotationSpeed = 0.001;
            const x = camera.position.x;
            const z = camera.position.z;
            camera.position.x = x * Math.cos(rotationSpeed) + z * Math.sin(rotationSpeed) - 10;
            camera.position.z = z * Math.cos(rotationSpeed) - x * Math.sin(rotationSpeed) - 10;
            camera.lookAt(new THREE.Vector3(0, 8000, 0));

            // update waves
            for (let i = 0; i < vertexCount; i++) {
                const z = Math.sin(i + count * 0.00002) * (initialZPositions[i] * 0.4);
                positionAttribute.setZ(i, z);
                count += 0.1;
            }
            positionAttribute.needsUpdate = true;

            renderer.render(scene, camera);
        };

        // start
        animate();

        const onWindowResize = () => {
            camera.aspect = window.innerWidth / window.innerHeight;
            camera.updateProjectionMatrix();

            renderer.setSize(window.innerWidth, window.innerHeight);
        };
        window.addEventListener('resize', onWindowResize, false);

        // cleanup
        return () => {
            cancelAnimationFrame(animationFrameId);
            window.removeEventListener('resize', onWindowResize);
            containerRef.current?.removeChild(renderer.domElement);
            renderer.dispose();
        };
    }, []);

    // cover the whole screen
    const styles: React.CSSProperties = {
        position: 'fixed',
        top: 0,
        left: 0,
        width: '100%',
        height: '100%',
        zIndex: -1,
    };

    return <div ref={containerRef} style={styles} />;
}